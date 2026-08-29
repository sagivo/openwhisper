# Default local LLM refiner

**Question:** which local GGUF should OpenWhisper use to turn a raw Whisper transcript into clean text — small, fast, low RAM. Not a general chatbot.

**Date:** 2026-08-29

**Recommendation for this job:** `s1-mini-q4_k_m.gguf` from [superwhisper/s1-mini-GGUF](https://huggingface.co/superwhisper/s1-mini-GGUF) — **462 MB**, 0.6B, purpose-built ASR normalizer. 94.8% token accuracy on 7,519 English cases. English only.

**If we will not ship a Superwhisper-named model:** `Qwen3-4B-Instruct-2507-Q4_K_M.gguf` (~2.33 GiB) stays the best general instruct default. Next-smaller general option is `Qwen3-1.7B` Q4_K_M (~1.11 GB), but it thinks by default and is a chatbot, not a normalizer.

Do not use Qwen3.5, LFM2.5-2.6B, or Gemma 4 as the refiner — they are thinking/agent models and waste the 500 ms budget.

---

## 2026-08-29 addendum: “just clean the transcript”

The product need is filler-stripping + punctuation + caps, not reasoning. Ranked for that:

| Model | Disk Q4_K_M | Why |
| --- | --- | --- |
| **S1-mini** | **462 MB** | Fine-tuned Qwen3-0.6B for this exact pipeline (`audio → ASR → S1-mini → clean text`). Greedy decode. Must send their system prompt + control line and `enable_thinking=false`. License: Apache 2.0 **plus** must keep the name “S1-mini” by “Superwhisper”. [card](https://huggingface.co/superwhisper/s1-mini) [GGUF](https://huggingface.co/superwhisper/s1-mini-GGUF) |
| Qwen3-1.7B | ~1.11 GB | Small general instruct. Thinking **on** by default — must disable. Will invent/chat if the prompt is loose. [card](https://huggingface.co/Qwen/Qwen3-1.7B) [GGUF](https://huggingface.co/unsloth/Qwen3-1.7B-GGUF) |
| Qwen3-4B-Instruct-2507 | 2.33 GiB | Non-thinking by design. Overkill on size, safest general rewrite. [card](https://huggingface.co/Qwen/Qwen3-4B-Instruct-2507) |
| LFM2.5-2.6B | 1.67 GB | Fast on-device (220 tok/s M5 Max) but **always thinks**. Wrong job. [card](https://huggingface.co/LiquidAI/LFM2.5-2.6B) |

S1-mini integration notes (from their card, not optional):

- System prompt and `[Styling: semi-formal] [Structure: prose] [Context: general]` control line are part of the trained format. Skip them and it garbles.
- llama.cpp: `--jinja --chat-template-kwargs '{"enable_thinking":false}' --temp 0`. Do not use `--reasoning-budget 0`.
- Empty output is valid (filler-only audio).

---

The rest of this note is the earlier general-LLM comparison (keep 4B Instruct if not taking S1-mini).

**Working-tree note:** `src-tauri/src/models.rs` and `scripts/download-models.sh` currently point at `unsloth/Qwen3.5-4B-GGUF` Q4_K_M. That is a stronger *thinking* model on paper and a worse default for this product.

---

## Constraints from this repo

OpenWhisper is a menu-bar dictation app: click-to-talk, short clips (seconds), Whisper.cpp STT, then a resident local LLM that rewrites the raw transcript into a clean message (strip um/uh/like, fix punctuation/caps, keep meaning) and pastes it. Fast-paste shows the raw Whisper text immediately, then replaces it with the refined version.

The LLM path is `llama-cpp-2` wrapping llama.cpp. Models load once at startup and stay resident next to Whisper. Refinement is a single-shot generation (`N_CTX = 4096`, `MAX_NEW_TOKENS = 256`) via `apply_chat_template` (Jinja / OpenAI-compat path in `llm_engine.rs`). Target: under ~500 ms on Apple Silicon Metal.

| | |
| --- | --- |
| STT default | `ggml-small.en.bin`, 487,614,201 bytes (~466 MiB) |
| Engine | llama.cpp via `llama-cpp-2` |
| Crate pin (working tree) | `llama-cpp-2 = 0.1.146` with a vendored `llama-cpp-sys-2` that forces `cparams.auto_fgdn = false` |
| Question’s stated pin | 0.1.133 (older; predates Gemma 4 ISWA / Qwen3.5 FGDN work) |
| Chat template | GGUF metadata, thinking disabled when the Jinja path is used (`enable_thinking: false`) |
| Thinking strip | engine already strips `<think>...</think>` if a swapped GGUF emits it |
| Disk budget | prefer ≤ ~3 GB Q4_K_M next to Whisper on a 16 GB Mac. Gemma 4 E4B Q4_K_M was abandoned as too heavy |
| Download | must be anonymously fetchable from Hugging Face |
| Job | English dictation cleanup. Multilingual is a bonus |

Wired in `scripts/download-models.sh`, `src-tauri/src/models.rs`, `src-tauri/src/config.rs`, and `src-tauri/src/llm_engine.rs`. Release builds on macOS enable Metal (`npm run release`).

---

## What “best” means here

Rank, in order:

1. Instruction-following on a short rewrite: filler stripping, punctuation, not inventing content, not being chatty
2. Latency for ~50–150 output tokens on Metal at Q4_K_M (or similar). Thinking traces are poison for the 500 ms budget
3. Disk + RAM next to Whisper small.en on 16 GB
4. llama.cpp maturity / GGUF availability / ungated download
5. License (Apache/MIT preferred over Gemma/Llama if quality is similar)

No primary source publishes Metal tok/s for these exact GGUFs on Apple Silicon. Latency claims below are architectural (thinking on/off, dense vs hybrid, param count), not measured numbers. Where a millisecond figure cannot be verified, it is marked as such.

---

## Disk sizes (HEAD, 2026-08-29)

`Content-Length` from Hugging Face after following the CDN redirect. Anonymous (`user_id=public`) unless noted.

| File | Bytes | GiB (1024) | Listed on card |
| --- | --- | --- | --- |
| `unsloth/.../Qwen3-4B-Instruct-2507-Q4_K_M.gguf` | **2,497,281,120** | **2.33** | 2.5 GB |
| `unsloth/.../Qwen3.5-4B-Q4_K_M.gguf` | 2,740,937,888 | 2.55 | 2.74 GB |
| `unsloth/.../gemma-4-E2B-it-Q4_K_M.gguf` | 3,106,738,272 | 2.89 | — |
| `unsloth/.../gemma-4-E4B-it-Q4_K_M.gguf` | 4,977,171,584 | 4.64 | ~5 GB (this repo’s earlier trial) |
| `bartowski/.../microsoft_Phi-4-mini-instruct-Q4_K_M.gguf` | 2,491,874,688 | 2.32 | — |
| `bartowski/.../Llama-3.2-3B-Instruct-Q4_K_M.gguf` | 2,019,377,696 | 1.88 | — |
| `bartowski/.../google_gemma-3-4b-it-Q4_K_M.gguf` | 2,489,758,112 | 2.32 | — |
| `unsloth/.../Ministral-3-3B-Instruct-2512-Q4_K_M.gguf` | 2,146,497,824 | 2.00 | — |
| `unsloth/.../Qwen3-1.7B-Q4_K_M.gguf` | 1,107,409,472 | 1.03 | — |
| `unsloth/.../SmolLM3-3B-Q4_K_M.gguf` | 1,915,306,528 | 1.78 | — |
| `ibm-granite/granite-3.3-2b-instruct-GGUF` Q4_K_M | 1,545,303,328 | 1.44 | — |

401 (not anonymously downloadable, or repo missing): `unsloth/Qwen3-1.7B-Instruct-2507-GGUF`, `ggml-org/Qwen3-4B-Instruct-2507-GGUF`, `ggml-org/Qwen3.5-4B-GGUF`, `ibm-granite/granite-4.0-tiny-instruct`. Qwen does not publish official GGUFs; Unsloth / Bartowski conversions are the shippable files.

---

## Candidates that stay on llama.cpp

### Qwen3-4B-Instruct-2507 (current default — keep)

Primary sources: [Qwen/Qwen3-4B-Instruct-2507](https://huggingface.co/Qwen/Qwen3-4B-Instruct-2507), [unsloth GGUF](https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF), [Qwen3 blog](https://qwenlm.github.io/blog/qwen3/), [arXiv:2505.09388](https://arxiv.org/abs/2505.09388).

| | |
| --- | --- |
| Params | 4.0B (3.6B non-embedding). Dense. 36 layers, GQA 32/8 |
| Context | 262,144 native (we use 4096) |
| License | Apache 2.0 |
| Gated | No. Unsloth GGUF HEAD returned 200 with `user_id=public` |
| Thinking | **None.** Authors: “This model supports only non-thinking mode and does not generate `<think></think>` blocks… specifying `enable_thinking=False` is no longer required.” |
| Arch in GGUF | `qwen3` — the architecture llama.cpp has supported since Qwen3’s April 2025 release. No Gated DeltaNet |
| Chat template | Standard Qwen3 instruct template in GGUF metadata |

Authors’ published numbers for **Instruct-2507 vs original Qwen3-4B non-thinking**:

| Bench | Qwen3-4B non-thinking | **Instruct-2507** |
| --- | --- | --- |
| IFEval | 81.2 | **83.4** |
| WritingBench | 68.5 | **83.4** |
| Creative Writing v3 | 53.6 | **83.5** |
| Arena-Hard v2 | 9.5 | **43.4** |
| MMLU-Pro | 58.0 | **69.6** |

The July 2025 instruct refresh is a writing/alignment jump, not a math-only bump. That is the axis this product cares about: follow a short “rewrite this transcript, do not invent” system prompt without getting chatty.

Qwen3 dense models are Apache 2.0. The April 2025 blog says even Qwen3-4B can rival Qwen2.5-72B-Instruct on the authors’ charts; Instruct-2507 is the non-thinking 4B they actually want people to ship.

### Qwen3.5-4B (working-tree default — do not ship)

Primary sources: [Qwen/Qwen3.5-4B](https://huggingface.co/Qwen/Qwen3.5-4B), [unsloth GGUF](https://huggingface.co/unsloth/Qwen3.5-4B-GGUF), citation `qwen.ai/blog?id=qwen3.5` (February 2026).

| | |
| --- | --- |
| Params | Authors: 4B. HF safetensors listing: 5B. Dense hybrid, not MoE at this size |
| Arch | “Causal Language Model with Vision Encoder.” Hidden layout: `8 × (3 × (Gated DeltaNet → FFN) → 1 × (Gated Attention → FFN))`. Vocab 248,320 |
| Context | 262,144 native |
| License | Apache 2.0 |
| Gated | No (Unsloth GGUF anonymous 200) |
| Thinking | **On by default.** Authors: “Qwen3.5 models operate in thinking mode by default, generating thinking content signified by `<think>\n...</think>\n\n`”. “Qwen3.5 does not officially support the soft switch of Qwen3, i.e., `/think` and `/nothink`.” Disable only via `chat_template_kwargs: enable_thinking: False` |
| llama.cpp | GGUF architecture `qwen35`. llama.cpp `docs/ops.md`: `GATED_DELTA_NET` on Metal is **🟡 partial**, on CUDA **❌**. Current llama.cpp README demos `ggml-org/Qwen3.5-0.8B-GGUF`, so *current* llama.cpp loads it; 0.1.133 would not. This repo’s 0.1.146 vendor sets `auto_fgdn = false` and the engine comment says Qwen3.5 then “still runs on the unfused path” |

Authors’ IFEval **89.8** / MMLU-Pro **79.1** are for the post-trained model in its default (thinking) regime. They are not a non-thinking rewrite score. Turning thinking off to hit 500 ms means we do not get those numbers, and we still pay for a hybrid + vision architecture we never use.

Disk 2.55 GiB is inside the 3 GB prefer-cap, but that is not the problem. The problem is: thinking-by-default vs a 256-token cap; a new arch with partial Metal ops; Jinja `enable_thinking` as a load-bearing flag. Instruct-2507 needs none of that.

### Gemma 4 E2B / E4B (already tried)

Primary source: [google/gemma-4-E2B-it](https://huggingface.co/google/gemma-4-E2B-it), [arXiv:2607.02770](https://arxiv.org/abs/2607.02770).

| | E2B | E4B |
| --- | --- | --- |
| Effective params | 2.3B (5.1B with embeddings / PLE) | 4.5B (8B with embeddings) |
| Layers / SWA | 35 / 512-token sliding window | 42 / 512 |
| Context | 128K | 128K |
| License | **Apache 2.0** (Gemma 4; Gemma 3 was the gated Gemma license) | same |
| MMLU Pro (it) | 60.0% | 69.4% |
| Q4_K_M disk | 3.11 GB / 2.89 GiB | 4.98 GB / 4.64 GiB |
| Thinking | Configurable. Trigger is `<\|think\|>` in the system prompt. Roles are `system`/`user`/`assistant`. Output uses `<\|channel>thought` | same |
| Modalities | Text, image, audio | Text, image, audio |

E4B Q4_K_M is the ~5 GB file this repo already abandoned. E2B’s GGUF is large *because of PLE embedding tables*, so the “2.3B effective” pitch does not deliver a 2 GB download. Hybrid SWA is exactly why llama-cpp-2 got pinned (Gemma 2/4 ISWA / FGDN abort). Chat-template flux is documented in `llm_engine.rs`. MMLU Pro 60.0 (E2B) is behind Instruct-2507’s 69.6; E4B ties that score and blows the disk budget.

Gemma 4 is a fine optional Browse… model. It is not the default.

### Gemma 3 4B IT

Primary source: [google/gemma-3-4b-it](https://huggingface.co/google/gemma-3-4b-it). Gated Gemma license click-through on the official repo. Multimodal, 128K context (4B). Pretrain MMLU 59.6 (5-shot). Q4_K_M 2.32 GiB (Bartowski, anonymous). Older than Gemma 4, gated, no reason to prefer it over Instruct-2507.

### Llama 3.2 3B Instruct

Primary source: [meta-llama/Llama-3.2-3B-Instruct](https://huggingface.co/meta-llama/Llama-3.2-3B-Instruct).

| | |
| --- | --- |
| Params | 3.21B dense, GQA, 128K context |
| License | Llama 3.2 Community License. **Gated** (contact-info click-through) |
| Official IFEval | 77.4 (bf16 instruct) vs Instruct-2507 **83.4** |
| Knowledge cutoff | December 2023 |
| Q4_K_M | 1.88 GiB (Bartowski, anonymous even though the base repo is gated) |

Smaller and faster in principle. Weaker instruction following on Meta’s own IFEval, gated official weights, custom license with “Built with Llama” redistribution terms. Not the default.

### Llama 4 Scout

Primary source: [meta-llama/Llama-4-Scout-17B-16E-Instruct](https://huggingface.co/meta-llama/Llama-4-Scout-17B-16E-Instruct). MoE, **17B active / 109B total**. Gated Llama 4 license. Not local-sized. Out.

### Phi-4-mini-instruct

Primary source: [microsoft/Phi-4-mini-instruct](https://huggingface.co/microsoft/Phi-4-mini-instruct), [arXiv:2503.01743](https://arxiv.org/abs/2503.01743).

| | |
| --- | --- |
| Params | 3.8B dense, GQA, shared embeddings, 128K context, vocab 200,064 |
| License | **MIT**. Ungated |
| Thinking | No. Chat format `<\|system\|>...<\|end\|><\|user\|>...<\|end\|><\|assistant\|>` |
| Q4_K_M | 2.32 GiB (Bartowski) |

Microsoft’s own table (same card): Arena Hard **32.8** vs Qwen2.5-3B-Ins 32.0 vs Llama-3.2-3B-Ins 17.0; overall 63.5 vs Qwen2.5-7B 67.9. They do not compare against Qwen3-4B-Instruct-2507 (Arena Hard 43.4, WritingBench 83.4). Closest MIT/Apache runner-up on packaging. Weaker published instruction/alignment numbers than Instruct-2507, custom special-token template. Keep as a documented MIT alternative, not the default.

### Ministral 3 3B Instruct 2512

Primary source: [mistralai/Ministral-3-3B-Instruct-2512](https://huggingface.co/mistralai/Ministral-3-3B-Instruct-2512), [arXiv:2601.08584](https://arxiv.org/abs/2601.08584), [Mistral 3 blog](https://mistral.ai/news/mistral-3).

| | |
| --- | --- |
| Params | 3.4B language + 0.4B vision encoder |
| License | Apache 2.0 |
| Context | 256K |
| Thinking | Instruct variant is non-reasoning (separate Reasoning-2512 exists) |
| Q4_K_M | 2.00 GiB (Unsloth, anonymous) |

Mistral’s instruct table: Arena Hard **0.305** vs Qwen3-VL-4B-Instruct 0.438 vs Gemma3-4B-Instruct 0.318. Listed use cases include “short content generation” and edge deployment. Vision encoder is unused here. Tokenizer is Mistral-specific (`mistral_common`). Weaker Arena Hard than Instruct-2507’s 43.4 on Qwen’s (different) Arena Hard v2. Not enough of a quality case to switch.

### SmolLM3-3B / SmolLM2-1.7B

Primary sources: [HuggingFaceTB/SmolLM3-3B](https://huggingface.co/HuggingFaceTB/SmolLM3-3B), [HuggingFaceTB/SmolLM2-1.7B-Instruct](https://huggingface.co/HuggingFaceTB/SmolLM2-1.7B-Instruct). Apache 2.0.

SmolLM3: 3B, hybrid reasoning **on by default**, disable via `/no_think` or `enable_thinking=False`. Authors’ no-think IFEval **76.7** vs their Qwen3-4B figure 68.9 (different methodology than Qwen’s 83.4 for Instruct-2507). Q4_K_M 1.78 GiB. Thinking default is the same latency trap as Qwen3.5.

SmolLM2-1.7B-Instruct: IFEval 56.7, Apache 2.0, has a first-party *text rewriting* example — but that IFEval is far below Instruct-2507. Too small to be the quality default.

### Qwen3-1.7B (hybrid thinking)

Primary source: [Qwen/Qwen3-1.7B](https://huggingface.co/Qwen/Qwen3-1.7B). 1.7B dense, Apache 2.0, **thinking on by default** (`enable_thinking=True`). Q4_K_M 1.03 GiB. There is no anonymously downloadable `Qwen3-1.7B-Instruct-2507` GGUF (Unsloth 401). A 1.7B thinking model is a low-RAM escape hatch, not a quality default.

### Granite / OLMo

- IBM Granite 4.0 tiny official card: **401 gated**. Unsloth `granite-4.0-h-tiny` Q4_K_M is 4.25 GB — over budget.
- Granite 3.3 2B Instruct GGUF Q4_K_M: 1.44 GiB, anonymous. Older / smaller; no published IFEval on the files we fetched that beats Instruct-2507.
- [allenai/OLMo-2-0425-1B-Instruct](https://huggingface.co/allenai/OLMo-2-0425-1B-Instruct): Apache 2.0, 1B, IFEval 70.1 on Ai2’s table. Too small.

---

## llama.cpp / this engine

- llama.cpp Metal is a first-class backend ([README](https://github.com/ggml-org/llama.cpp/blob/master/README.md)).
- `GATED_DELTA_NET` Metal support is partial ([ops.md](https://github.com/ggml-org/llama.cpp/blob/master/docs/ops.md)). That is Qwen3.5’s linear-attention op. Instruct-2507 does not use it.
- `llm_engine.rs` already (1) renders chat templates with `enable_thinking: false` on the Jinja path, (2) falls back to the C applicator if Jinja fails, (3) strips `<think>...</think>`. That is the right defensive code for *optional* thinking models. It is not a reason to *ship* a thinking-default model: if Jinja fails, Qwen3.5 thinks; Instruct-2507 never does.
- Question mentioned 0.1.133 because newer llama-cpp-2 asserted on Gemma 2 SWA. Working tree is 0.1.146 plus a one-line FGDN patch so Gemma 2/4 still load. Instruct-2507 does not need that patch.

---

## Decision

**Default going forward: `Qwen3-4B-Instruct-2507-Q4_K_M.gguf`**

```
https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf
```

Disk **2,497,281,120 bytes (2.33 GiB)**. Unsloth lists 2.5 GB. Apache 2.0. Ungated.

Why this one, given the constraints:

1. **The job is a short non-thinking rewrite.** Instruct-2507 is the Qwen3-4B *non-thinking* instruct checkpoint. Authors forbid `<think>` blocks. WritingBench 83.4 / Creative Writing v3 83.5 / IFEval 83.4 are measured in that mode. No other 2–3 GB GGUF in this set publishes a stronger writing/IF number without thinking.
2. **Latency architecture matches the 500 ms budget.** No reasoning trace to eat the 256-token cap. Dense `qwen3` on Metal, not partial Gated DeltaNet. (Exact Metal milliseconds: not in any primary source we fetched.)
3. **Fits next to Whisper small.en.** 2.33 GiB + 466 MiB is well under the ~3 GB prefer-cap that killed Gemma 4 E4B (~5 GB).
4. **Packaging is boring in the good way.** Apache 2.0, anonymous HF download, chat template in GGUF metadata, llama.cpp support since Qwen3 day one. No license click-through, no vision encoder, no `/think` switch.
5. **Qwen3.5’s headline scores are the wrong mode.** IFEval 89.8 is a thinking-default multimodal 4B. Disabling thinking to stay fast throws those scores out; keeping thinking blows the latency target. Hybrid + 248k vocab + partial Metal GDN is cost with no product benefit.

What we are not picking, and why:

| Option | Verdict |
| --- | --- |
| Switch to Qwen3.5-4B Q4_K_M (working-tree URL) | Better thinking benches, worse default. Thinks unless Jinja `enable_thinking=false` sticks. New `qwen35` arch, partial Metal GDN, unused vision. 2.55 GiB. Revert the download URL. |
| Gemma 4 E4B Q4_K_M | Already tried. 4.98 GB. Too heavy on 16 GB next to Whisper. |
| Gemma 4 E2B Q4_K_M | Apache 2.0 now, but 3.11 GB because of PLE embeddings, hybrid SWA, thinking tokens, MMLU Pro 60.0 vs 69.6. Optional Browse… model. |
| Gemma 3 4B IT | Gated Gemma license. Older. No quality case over Instruct-2507. |
| Phi-4-mini Q4_K_M | Best MIT alternative (2.32 GiB, ungated, no thinking). Arena Hard 32.8 vs Instruct-2507 43.4. Custom `<\|end\|>` template. Document, don’t default. |
| Ministral 3 3B Instruct | Apache 2.0, 2.00 GiB, non-thinking instruct. Arena Hard 0.305 on Mistral’s table. Vision unused. Not stronger on the rewrite axis. |
| Llama 3.2 3B Instruct | Gated, IFEval 77.4, Llama license. Smaller, not better. |
| Llama 4 Scout | 17B active / 109B total. Not a laptop default. |
| SmolLM3-3B | Apache, 1.78 GiB, thinking on by default. No-think IFEval 76.7 on HF’s table. Fast-path candidate only if we ever need a 2 GB cap. |
| Qwen3-1.7B / SmolLM2-1.7B / OLMo-2 1B / Granite 3.3 2B | Too small on published IF/quality. Granite 4 tiny official is gated; the tiny GGUF we found is 4.25 GB. |
| Stay on Gemma 2 2B IT | Already left. Unsloth GGUF 401 today. |

### Suggested follow-ups (not blockers)

1. Point `ModelKind::Llm` and `download-models.sh` back at Instruct-2507 if the working tree still downloads Qwen3.5.
2. Keep the Jinja `enable_thinking: false` path and `<think>` strip — they make Browse… of Qwen3.5 / SmolLM3 / Gemma 4 safe.
3. Keep Browse… so Phi-4-mini (MIT) and SmolLM3-3B (smaller) remain one-click.
4. Do not bump llama-cpp-2 just to chase Qwen3.5 fused GDN on Metal. Instruct-2507 does not need it.

---

## Sources

- Qwen3-4B-Instruct-2507 card (params, non-thinking note, bench table, Apache 2.0): https://huggingface.co/Qwen/Qwen3-4B-Instruct-2507
- Unsloth Instruct-2507 GGUF (qwen3 arch, Q4_K_M 2.5 GB listed, Apache 2.0): https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF
- Qwen3 blog (dense sizes, Apache 2.0, thinking/non-thinking, llama.cpp listed): https://qwenlm.github.io/blog/qwen3/
- Qwen3 technical report: https://arxiv.org/abs/2505.09388
- Qwen3.5-4B card (hybrid Gated DeltaNet, thinking default, no `/think` switch, IFEval 89.8, Apache 2.0): https://huggingface.co/Qwen/Qwen3.5-4B
- Unsloth Qwen3.5-4B GGUF (qwen35 arch, Q4_K_M 2.74 GB listed): https://huggingface.co/unsloth/Qwen3.5-4B-GGUF
- Gemma 4 E2B-it card (Apache 2.0, PLE, thinking tokens, MMLU Pro table): https://huggingface.co/google/gemma-4-E2B-it
- Gemma 4 technical report: https://arxiv.org/abs/2607.02770
- Gemma 3 4B IT card (gated Gemma license, 128K, MMLU 59.6 PT): https://huggingface.co/google/gemma-3-4b-it
- Llama 3.2 3B Instruct card (gated, IFEval 77.4, 3.21B): https://huggingface.co/meta-llama/Llama-3.2-3B-Instruct
- Llama 4 Scout card (17B×16E, 109B total, gated): https://huggingface.co/meta-llama/Llama-4-Scout-17B-16E-Instruct
- Phi-4-mini-instruct card (3.8B, MIT, Arena Hard 32.8, chat format): https://huggingface.co/microsoft/Phi-4-mini-instruct
- Phi-4-mini technical report: https://arxiv.org/abs/2503.01743
- Ministral 3 3B Instruct 2512 (Apache 2.0, 3.4B+0.4B vision, Arena Hard 0.305): https://huggingface.co/mistralai/Ministral-3-3B-Instruct-2512
- Ministral 3 paper: https://arxiv.org/abs/2601.08584
- SmolLM3-3B (Apache 2.0, thinking default, no-think IFEval 76.7): https://huggingface.co/HuggingFaceTB/SmolLM3-3B
- SmolLM2-1.7B-Instruct (IFEval 56.7, rewrite example): https://huggingface.co/HuggingFaceTB/SmolLM2-1.7B-Instruct
- Qwen3-1.7B (thinking default): https://huggingface.co/Qwen/Qwen3-1.7B
- OLMo 2 1B Instruct (IFEval 70.1, Apache 2.0): https://huggingface.co/allenai/OLMo-2-0425-1B-Instruct
- llama.cpp README (Metal first-class, Qwen3.5-0.8B demo): https://github.com/ggml-org/llama.cpp/blob/master/README.md
- llama.cpp ops.md (`GATED_DELTA_NET` Metal 🟡): https://github.com/ggml-org/llama.cpp/blob/master/docs/ops.md
- Disk sizes: Hugging Face HEAD `Content-Length` on the resolve URLs above, 2026-08-29
- This repo: `scripts/download-models.sh`, `src-tauri/src/models.rs`, `src-tauri/src/config.rs`, `src-tauri/src/llm_engine.rs`, `src-tauri/Cargo.toml`
