//! Local LLM (Gemma 4) refinement using llama.cpp bindings.
//!
//! Loads a GGUF model once, then `refine(raw)` runs a short greedy generation
//! that rewrites the raw transcription into a clean message.
//!
//! ## Why we use `apply_chat_template` instead of formatting by hand
//!
//! Earlier versions of this file hand-rolled the Gemma 2 chat template
//! (`<start_of_turn>user\n...<end_of_turn>\n<start_of_turn>model\n`). That
//! worked because Gemma 2's template was small, stable, and a single
//! `<end_of_turn>` literal was a reliable stop marker.
//!
//! Gemma 4's template is fundamentally different:
//!
//!   * Roles are now standard `system` / `user` / `assistant` (not
//!     turn-based `user` / `model`).
//!   * Turns are wrapped with `<|turn|>` and a `<|channel>` tag is used to
//!     interleave reasoning and final answers when the optional thinking
//!     mode is enabled (`<|think|>` in the system prompt).
//!   * The template is *still in flux upstream* — the unsloth GGUF repo
//!     notes "New Gemma chat template update by Google" days before this
//!     was written.
//!
//! Hand-rolling that template would be both fragile and a source of subtle
//! refinement-quality bugs (wrong special tokens cause the model to start
//! its reply with stray `<|channel>thought\n` markers). Instead we let
//! llama.cpp format the prompt from the chat template baked into the GGUF
//! metadata. The trade-off is we can no longer do per-system-prompt KV
//! caching — but refinement is single-turn anyway, so the win was modest
//! to begin with (~30-50 ms saved per call on Metal). End-of-generation is
//! detected via `is_eog_token`, which covers all of Gemma 4's stop tokens
//! (`<|turn>`, `<|eot|>`, EOS) without us hardcoding any literal.
//!
//! Performance notes:
//! - The `LlamaContext` is built once at load time and reused across calls.
//!   Between refinements we drop the KV cache rather than re-allocating the
//!   whole context (which costs ~100ms even on Metal).

use anyhow::{anyhow, bail, Context, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::num::NonZeroU32;
use std::path::Path;
use std::pin::pin;

static BACKEND: OnceCell<LlamaBackend> = OnceCell::new();

fn backend() -> Result<&'static LlamaBackend> {
    BACKEND.get_or_try_init(|| LlamaBackend::init().map_err(|e| anyhow!("llama backend: {e}")))
}

const N_CTX: u32 = 4096;
const MAX_NEW_TOKENS: i32 = 256;

struct Inner {
    model: &'static LlamaModel,
    ctx: LlamaContext<'static>,
    /// Chat template baked into the GGUF metadata. Cached once at load time
    /// because looking it up traverses model metadata.
    chat_template: LlamaChatTemplate,
}

/// Owns the `Box<LlamaModel>` that was leaked to get a `'static` reference for
/// `LlamaContext`. When `ModelOwner` is dropped it reconstructs and frees the
/// box via `Box::from_raw`.
///
/// # Safety
/// `LlmEngine` declares `inner` (containing `ctx`) **before** `_model_owner`,
/// so Rust's field-drop order guarantees `ctx` is freed before the model
/// allocation is reclaimed — satisfying the borrow that ties `ctx` lifetime to
/// the model.
struct ModelOwner(*mut LlamaModel);

// SAFETY: the pointer is heap-allocated and exclusively owned by ModelOwner.
unsafe impl Send for ModelOwner {}
unsafe impl Sync for ModelOwner {}

impl Drop for ModelOwner {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: constructed via Box::new + Box::leak; LlamaContext has
            // already been dropped (it lives in `inner`, declared first).
            unsafe { drop(Box::from_raw(self.0)) };
        }
    }
}

pub struct LlmEngine {
    inner: Mutex<Inner>, // dropped first — frees LlamaContext which borrows the model
    _model_owner: ModelOwner, // dropped second — frees the Box<LlamaModel> allocation
}

// SAFETY: `LlamaContext` is `!Send + !Sync` purely because it wraps a raw
// `NonNull` pointer. The underlying llama.cpp context is safe to call from a
// single thread at a time, which the Mutex guarantees. The `&'static
// LlamaModel` is genuinely 'static (we leak the box), so cross-thread access
// is fine.
unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

impl LlmEngine {
    pub fn load(model_path: &Path, n_threads: i32) -> Result<Self> {
        if !model_path.exists() {
            return Err(anyhow!("LLM model not found: {}", model_path.display()));
        }
        let backend = backend()?;
        let model_params = pin!(LlamaModelParams::default());
        let model = LlamaModel::load_from_file(backend, model_path, &model_params)
            .context("loading llama model")?;

        // Leak the box to get a `'static` reference for LlamaContext. The raw
        // pointer is kept in `_model_owner` so it can be freed on drop. Field
        // order in `LlmEngine` guarantees `inner` (ctx) is dropped first.
        let model_box = Box::new(model);
        let model_ptr = &*model_box as *const LlamaModel as *mut LlamaModel;
        let model: &'static LlamaModel = Box::leak(model_box);

        let chat_template = model
            .chat_template(None)
            .context("read chat template from GGUF metadata")?;

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(N_CTX))
            .with_n_threads(n_threads.max(1));

        let ctx = model
            .new_context(backend, ctx_params)
            .context("create llama context")?;

        Ok(Self {
            inner: Mutex::new(Inner {
                model,
                ctx,
                chat_template,
            }),
            _model_owner: ModelOwner(model_ptr),
        })
    }

    pub fn refine(&self, system_prompt: &str, raw: &str) -> Result<String> {
        let mut inner = self.inner.lock();
        let Inner {
            model,
            ctx,
            chat_template,
        } = &mut *inner;

        let sys_trimmed = system_prompt.trim();
        let raw_trimmed = raw.trim();

        // Build a [system, user] conversation. Wrapping the transcript in
        // delimiters keeps the model from confusing the user instruction with
        // the speech content, which is the same trick we used pre-Gemma-4.
        let user_content = format!("--- TRANSCRIPTION ---\n{raw_trimmed}\n--- END ---");
        let messages = vec![
            LlamaChatMessage::new("system".to_string(), sys_trimmed.to_string())
                .context("build system message")?,
            LlamaChatMessage::new("user".to_string(), user_content)
                .context("build user message")?,
        ];

        // `add_ass = true` appends the assistant-turn opener so the next
        // sampled token is the model's first reply token.
        let prompt = model
            .apply_chat_template(chat_template, &messages, true)
            .context("apply chat template")?;

        // Reset KV cache for a clean single-shot generation. Gemma 4's
        // chat template is too involved to safely cache a per-system-prompt
        // prefix the way we did for Gemma 2, and refinement is one-shot
        // anyway.
        ctx.clear_kv_cache();

        // The chat template emits its own BOS, so we must NOT add another.
        let prompt_tokens = model
            .str_to_token(&prompt, AddBos::Never)
            .context("tokenize prompt")?;

        let n_prompt = prompt_tokens.len() as i32;
        if (n_prompt + MAX_NEW_TOKENS) as u32 > N_CTX {
            bail!(
                "prompt too long: prompt_tokens={} max_new={} ctx={}",
                n_prompt,
                MAX_NEW_TOKENS,
                N_CTX
            );
        }

        let batch_cap = prompt_tokens.len().max(512);
        let mut batch = LlamaBatch::new(batch_cap, 1);

        // Only the very last prompt token needs logits — that's where we
        // sample the model's first reply token from.
        let last_idx = prompt_tokens.len().saturating_sub(1);
        for (i, tok) in prompt_tokens.iter().enumerate() {
            batch.add(*tok, i as i32, &[0], i == last_idx)?;
        }
        ctx.decode(&mut batch).context("decode prompt")?;

        // Greedy decoding: cheap, deterministic, plenty good for cleanup.
        // Gemma 4's recommended sampling (temp=1.0, top_p=0.95, top_k=64)
        // is for chat use; for deterministic transcript cleanup, greedy
        // gives more predictable output and avoids the occasional rephrase.
        let mut sampler = LlamaSampler::greedy();

        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = n_prompt;
        let max_cur = n_cur + MAX_NEW_TOKENS;

        while n_cur < max_cur {
            let token = sampler.sample(ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            // `is_eog_token` covers EOS, `<|turn>` (Gemma 4 turn boundary),
            // `<|eot|>`, and any other model-declared end-of-generation
            // tokens. This is what replaces the old hardcoded
            // `<end_of_turn>` literal check.
            if model.is_eog_token(token) {
                break;
            }

            let piece = model
                .token_to_piece(token, &mut decoder, false, None)
                .unwrap_or_default();
            output.push_str(&piece);

            batch.clear();
            batch.add(token, n_cur, &[0], true)?;
            ctx.decode(&mut batch).context("decode token")?;
            n_cur += 1;
        }

        Ok(clean(&output))
    }
}

fn clean(s: &str) -> String {
    let s = s.trim();

    // Defense in depth: if the model ever produces a thinking-mode prefix
    // (it shouldn't, since we never put `<|think|>` in the system prompt
    // and Gemma 4 doesn't emit empty thought blocks when thinking is off),
    // drop it before showing the user the result. The model card documents
    // thoughts as `<|channel>thought\n...` followed by the final answer.
    let s = strip_thought_prefix(s);

    // Only strip a pair of matching wrapping quotes — never strip just one
    // side, otherwise an internal quote like `He said "hi"` would lose its
    // closing mark.
    let stripped = strip_wrapping(s, '"', '"')
        .or_else(|| strip_wrapping(s, '\u{201C}', '\u{201D}'))
        .unwrap_or(s);

    stripped.trim().to_string()
}

fn strip_thought_prefix(s: &str) -> &str {
    // Matches `<|channel>thought\n...<|something|>` style leading thought
    // blocks. We're permissive: if we see a leading `<|channel>` tag we
    // skip everything up to the next `<|` token boundary that isn't part
    // of the thought header.
    if let Some(rest) = s.strip_prefix("<|channel>thought\n") {
        // Find the end of the thought: the next `<|` that starts a new tag.
        if let Some(end) = rest.find("<|") {
            return rest[end..]
                .trim_start_matches(|c: char| c != '>')
                .trim_start_matches('>')
                .trim_start();
        }
    }
    s
}

fn strip_wrapping(s: &str, open: char, close: char) -> Option<&str> {
    let mut chars = s.chars();
    let first = chars.next()?;
    let last = s.chars().next_back()?;
    if first == open && last == close && s.chars().count() >= 2 {
        let after_first = first.len_utf8();
        let before_last = s.len() - last.len_utf8();
        if after_first <= before_last {
            return Some(&s[after_first..before_last]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_strips_surrounding_whitespace() {
        assert_eq!(clean("  hello world  \n"), "hello world");
    }

    #[test]
    fn clean_strips_ascii_quotes() {
        assert_eq!(clean("\"hello world\""), "hello world");
    }

    #[test]
    fn clean_strips_smart_quotes() {
        assert_eq!(clean("\u{201C}hello world\u{201D}"), "hello world");
    }

    #[test]
    fn clean_preserves_internal_quotes() {
        assert_eq!(clean("He said \"hi\""), "He said \"hi\"");
    }

    #[test]
    fn clean_handles_empty_and_whitespace_only() {
        assert_eq!(clean(""), "");
        assert_eq!(clean("   "), "");
    }

    #[test]
    fn clean_strips_gemma4_thought_prefix() {
        let raw = "<|channel>thought\nThe user wants me to clean this up.<|message|>Hello world.";
        assert_eq!(clean(raw), "Hello world.");
    }
}
