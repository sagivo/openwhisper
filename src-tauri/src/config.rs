use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub hotkey: String,
    pub whisper_model_path: String,
    pub llm_model_path: String,
    pub refine_prompt: String,
    pub use_llm_refinement: bool,
    pub language: String,
    /// Paste the raw Whisper transcript as soon as it's ready, then replace
    /// it with the refined version when the LLM finishes. Drastically reduces
    /// perceived latency at the cost of a brief on-screen flash. Replacement
    /// is skipped if the refinement takes longer than a couple of seconds
    /// (so the user typing afterwards isn't clobbered).
    ///
    /// Off by default while we shake out edge cases — the classic
    /// refine-then-paste path is more thoroughly battle-tested. Users can
    /// opt in via Settings.
    #[serde(default)]
    pub fast_paste: bool,

    /// Restore the user's clipboard after pasting the transcript. Off by
    /// default because (a) the user just asked us to dictate something, so
    /// they expect the dictated text to be available for re-paste, and
    /// (b) silently swapping the clipboard back to old contents can hide
    /// our paste from the user when the Cmd+V didn't land in a text field
    /// for any reason. Enable in Settings if you'd rather we preserve your
    /// prior clipboard contents.
    #[serde(default)]
    pub restore_clipboard: bool,

    /// Maximum recording length in seconds. This bounds the in-memory audio
    /// buffer if a hotkey is accidentally held down or a release event is lost.
    #[serde(default = "default_max_recording_seconds")]
    pub max_recording_seconds: u32,

    /// Native inference threads for Whisper and the LLM. `0` means "auto" and
    /// uses the existing half-of-available-cores heuristic.
    #[serde(default)]
    pub inference_threads: u32,

    /// Seconds of inactivity after which loaded models are dropped from
    /// memory. Models reload (lazily) on the next dictation, which costs a
    /// couple of seconds of latency but frees ~1 GB of RAM. `0` disables
    /// unloading entirely (models stay resident once loaded).
    #[serde(default = "default_idle_unload_secs")]
    pub idle_unload_secs: u32,

    /// Show OpenWhisper in the Cmd+Tab app switcher (and the Dock). Off by
    /// default — this is a menu-bar app, so it stays out of the switcher
    /// unless the user opts in.
    #[serde(default)]
    pub show_in_app_switcher: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            whisper_model_path: String::new(),
            llm_model_path: String::new(),
            refine_prompt: DEFAULT_REFINE_PROMPT.to_string(),
            use_llm_refinement: true,
            language: "auto".to_string(),
            fast_paste: false,
            restore_clipboard: false,
            max_recording_seconds: default_max_recording_seconds(),
            inference_threads: 0,
            idle_unload_secs: default_idle_unload_secs(),
            show_in_app_switcher: false,
        }
    }
}

pub fn default_max_recording_seconds() -> u32 {
    120
}

pub fn default_idle_unload_secs() -> u32 {
    600
}

pub fn resolve_inference_threads(configured: u32) -> i32 {
    if configured > 0 {
        return configured.clamp(1, 32) as i32;
    }
    std::thread::available_parallelism()
        .map(|n| ((n.get() / 2).max(2)) as i32)
        .unwrap_or(2)
}

#[cfg(target_os = "macos")]
fn default_hotkey() -> String {
    "Fn".into()
}

#[cfg(not(target_os = "macos"))]
fn default_hotkey() -> String {
    "Fn".into()
}

pub const DEFAULT_REFINE_PROMPT: &str = "You are a dictation refinement assistant. Rewrite the following raw speech-to-text transcription as a clean, well-punctuated message. Remove filler words (um, uh, like, you know), false starts, and verbal tics. Preserve the speaker's intent, tone, and meaning exactly. Do not add new information. Output ONLY the rewritten message, with no preamble, no quotes, and no explanation.";

pub fn config_path() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("openwhisper");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("config.json")
}

pub fn load() -> Config {
    let path = config_path();
    let mut cfg = if let Ok(s) = std::fs::read_to_string(&path) {
        serde_json::from_str::<Config>(&s).unwrap_or_default()
    } else {
        Config::default()
    };

    // Auto-populate default model paths if the bundled/downloaded files exist
    // in the user's data dir but the config doesn't reference them yet.
    let data_models = dirs::data_dir()
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("openwhisper")
        .join("models");
    if cfg.whisper_model_path.is_empty() {
        // Prefer the current default (small.en); fall back to the older base.en
        // so existing installs aren't forced to re-download.
        for name in ["ggml-small.en.bin", "ggml-base.en.bin"] {
            let p = data_models.join(name);
            if p.exists() {
                cfg.whisper_model_path = p.to_string_lossy().into_owned();
                break;
            }
        }
    }
    if cfg.llm_model_path.is_empty() {
        // Prefer the current default (Qwen3 4B Instruct); fall back to older
        // GGUFs so existing installs aren't forced to re-download.
        for name in [
            "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
            "Qwen3.5-4B-Q4_K_M.gguf",
            "gemma-4-E4B-it-Q4_K_M.gguf",
            "gemma-4-E2B-it-Q4_K_M.gguf",
        ] {
            let p = data_models.join(name);
            if p.exists() {
                cfg.llm_model_path = p.to_string_lossy().into_owned();
                break;
            }
        }
    }
    cfg
}

pub fn save(cfg: &Config) -> std::io::Result<()> {
    let path = config_path();
    let s = serde_json::to_string_pretty(cfg).unwrap();
    std::fs::write(path, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_switcher_defaults_to_hidden() {
        assert!(!Config::default().show_in_app_switcher);
    }

    #[test]
    fn app_switcher_missing_from_json_stays_hidden() {
        let cfg: Config = serde_json::from_str(
            r#"{
                "hotkey": "Fn",
                "whisper_model_path": "",
                "llm_model_path": "",
                "refine_prompt": "x",
                "use_llm_refinement": true,
                "language": "auto"
            }"#,
        )
        .unwrap();
        assert!(!cfg.show_in_app_switcher);
    }
}
