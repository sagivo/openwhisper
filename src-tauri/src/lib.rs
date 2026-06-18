pub mod audio;
pub mod config;
pub mod injector;
pub mod llm_engine;
pub mod models;
pub mod whisper_engine;

use anyhow::{anyhow, Result};
use config::Config;
use crossbeam_channel::{bounded, Sender};
use parking_lot::Mutex;
use serde::Serialize;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tauri::image::Image;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState,
};

const TRAY_ID: &str = "main";

// Embedded tray icons. All icons are opaque-white waveform glyphs on a
// transparent background, used as macOS template images so the OS auto-tints
// them with the menu-bar foreground color (white in dark mode, black in light
// mode).
//
// While recording, we don't change the icon *color* — we cycle through a set
// of waveform frames whose bar heights vary, so the icon appears to "dance"
// like a live audio meter. Frames are advanced once per `update_tray` call
// while in the Recording state (driven by the existing 80–120ms level loop
// in `on_hotkey` / `toggle_dictation`).
const TRAY_ICON_TEMPLATE_PNG: &[u8] = include_bytes!("../icons/tray-mic-template.png");
const TRAY_ICON_RECORDING_PNGS: &[&[u8]] = &[
    include_bytes!("../icons/tray-mic-rec-00.png"),
    include_bytes!("../icons/tray-mic-rec-01.png"),
    include_bytes!("../icons/tray-mic-rec-02.png"),
    include_bytes!("../icons/tray-mic-rec-03.png"),
    include_bytes!("../icons/tray-mic-rec-04.png"),
    include_bytes!("../icons/tray-mic-rec-05.png"),
    include_bytes!("../icons/tray-mic-rec-06.png"),
    include_bytes!("../icons/tray-mic-rec-07.png"),
];

// Decoded-once icon cache. `Image::from_bytes` decodes a PNG into RGBA pixels,
// which we previously did on every tray update — i.e. ~12 PNG decodes per
// second while recording. Cache the decoded RGBA buffers as `Image<'static>`
// (the buffer is owned via `Cow::Owned`) and clone the cheap top-level struct
// when we need to hand it to Tauri.
static TRAY_ICON_TEMPLATE_CACHE: once_cell::sync::OnceCell<Image<'static>> =
    once_cell::sync::OnceCell::new();
static TRAY_ICON_RECORDING_CACHE: once_cell::sync::OnceCell<Vec<Image<'static>>> =
    once_cell::sync::OnceCell::new();

fn tray_icon_template() -> Option<Image<'static>> {
    TRAY_ICON_TEMPLATE_CACHE
        .get_or_try_init(|| Image::from_bytes(TRAY_ICON_TEMPLATE_PNG))
        .ok()
        .cloned()
}

fn tray_icon_recording_frame(frame: usize) -> Option<Image<'static>> {
    let cache = TRAY_ICON_RECORDING_CACHE.get_or_init(|| {
        TRAY_ICON_RECORDING_PNGS
            .iter()
            .filter_map(|b| Image::from_bytes(b).ok())
            .collect()
    });
    if cache.is_empty() {
        return None;
    }
    cache.get(frame % cache.len()).cloned()
}

use audio::Recorder;
use llm_engine::LlmEngine;
use whisper_engine::WhisperEngine;

/// Status payload mirrored to the frontend overlay.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Status {
    Idle,
    Recording { level: f32 },
    Transcribing,
    Refining,
    Injecting,
    Error { message: String },
}

/// Persistent engine readiness payload, emitted on load and whenever it
/// changes. The Settings UI binds to this so users can see at a glance what's
/// missing/broken instead of getting a generic error at dictation time.
#[derive(Clone, Default, Serialize)]
struct EngineStatus {
    whisper_loaded: bool,
    whisper_error: Option<String>,
    llm_loaded: bool,
    llm_error: Option<String>,
    hotkey: String,
    hotkey_registered: bool,
    hotkey_error: Option<String>,
}

/// Pipeline jobs handed off from the hotkey thread.
enum Job {
    Process(Vec<f32>),
    ReloadModels,
}

pub struct AppState {
    config: Mutex<Config>,
    recorder: Arc<Recorder>,
    whisper: Mutex<Option<Arc<WhisperEngine>>>,
    llm: Mutex<Option<Arc<LlmEngine>>>,
    job_tx: Mutex<Option<Sender<Job>>>,
    /// Tracks the currently registered hotkey for re-registration.
    current_hotkey: Mutex<Option<Shortcut>>,
    /// Monotonic frame counter used to pick the next tray-icon frame while
    /// recording; advanced on every `Status::Recording` tray update.
    rec_anim_frame: Mutex<usize>,
    /// Latest known engine readiness. Mirrored to the frontend whenever it
    /// changes via the `engine-status` event.
    engine_status: Mutex<EngineStatus>,
    /// Last tooltip text set on the tray icon. Used to skip redundant
    /// `set_tooltip` syscalls when the text hasn't changed.
    last_tray_tooltip: Mutex<&'static str>,
}

impl AppState {
    fn new() -> Self {
        Self {
            config: Mutex::new(config::load()),
            recorder: Arc::new(Recorder::new()),
            whisper: Mutex::new(None),
            llm: Mutex::new(None),
            job_tx: Mutex::new(None),
            current_hotkey: Mutex::new(None),
            rec_anim_frame: Mutex::new(0),
            engine_status: Mutex::new(EngineStatus::default()),
            last_tray_tooltip: Mutex::new(""),
        }
    }
}

fn emit_engine_status(app: &AppHandle) {
    let snapshot = app.state::<AppState>().engine_status.lock().clone();
    let _ = app.emit("engine-status", snapshot);
}

#[tauri::command]
fn get_engine_status(state: State<AppState>) -> EngineStatus {
    state.engine_status.lock().clone()
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_config(state: State<AppState>) -> Config {
    state.config.lock().clone()
}

#[tauri::command]
fn save_config(app: AppHandle, state: State<AppState>, config: Config) -> Result<(), String> {
    config::save(&config).map_err(|e| e.to_string())?;
    let old_hotkey = state.config.lock().hotkey.clone();
    *state.config.lock() = config.clone();
    if old_hotkey != config.hotkey {
        register_hotkey(&app, &config.hotkey).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn reload_models(state: State<AppState>) -> Result<(), String> {
    let tx = state.job_tx.lock().clone();
    if let Some(tx) = tx {
        tx.try_send(Job::ReloadModels).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn pick_file(app: AppHandle, kind: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (name, exts): (&str, Vec<&str>) = match kind.as_str() {
        "whisper" => ("Whisper GGML model", vec!["bin", "ggml"]),
        "llm" => ("GGUF model", vec!["gguf"]),
        _ => ("Model", vec![]),
    };
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter(name, &exts)
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(path.map(|p: PathBuf| p.to_string_lossy().into_owned()))
}

/// Test command: records `seconds` seconds from the default mic and runs the
/// full pipeline (transcription + refinement + text injection). Returns the
/// final text. Useful for verifying mic + Whisper + LLM + injection without
/// needing a hotkey press.
#[tauri::command]
async fn test_dictate(app: AppHandle, seconds: u64, inject: bool) -> Result<String, String> {
    let state = app.state::<AppState>();
    let recorder = state.recorder.clone();
    let whisper = state
        .whisper
        .lock()
        .clone()
        .ok_or_else(|| "Whisper not loaded".to_string())?;
    let llm = state.llm.lock().clone();
    let cfg = state.config.lock().clone();

    recorder
        .start(cfg.max_recording_seconds)
        .map_err(|e| e.to_string())?;
    emit_status(&app, Status::Recording { level: 0.0 });
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(Duration::from_secs(seconds));
    })
    .await
    .map_err(|e| e.to_string())?;

    let samples = recorder.stop().map_err(|e| e.to_string())?;
    emit_log(&app, &format!("Captured {} samples", samples.len()));

    emit_status(&app, Status::Transcribing);
    let raw = tauri::async_runtime::spawn_blocking(move || whisper.transcribe(&samples))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    emit_log(&app, &format!("Transcribed: {raw}"));

    let final_text = if cfg.use_llm_refinement {
        if let Some(llm) = llm {
            emit_status(&app, Status::Refining);
            let prompt = cfg.refine_prompt.clone();
            let raw_clone = raw.clone();
            match tauri::async_runtime::spawn_blocking(move || llm.refine(&prompt, &raw_clone))
                .await
                .map_err(|e| e.to_string())?
            {
                Ok(refined) if !refined.is_empty() => refined,
                _ => raw,
            }
        } else {
            raw
        }
    } else {
        raw
    };

    emit_log(&app, &format!("Final: {final_text}"));
    if inject {
        emit_status(&app, Status::Injecting);
        injector::type_text(final_text.clone(), cfg.restore_clipboard)
            .map_err(|e| e.to_string())?;
    }
    emit_status(&app, Status::Idle);
    Ok(final_text)
}

#[tauri::command]
fn list_models(state: State<AppState>) -> Vec<models::ModelStatus> {
    let cfg = state.config.lock().clone();
    vec![
        models::status_with_config(models::ModelKind::Whisper, &cfg.whisper_model_path),
        models::status_with_config(models::ModelKind::Llm, &cfg.llm_model_path),
    ]
}

#[tauri::command]
async fn download_model(app: AppHandle, kind: models::ModelKind) -> Result<String, String> {
    // If the user already has a valid model file (either at the default
    // download location or at a custom configured path), skip the download
    // and just return the existing path.
    let configured = {
        let state = app.state::<AppState>();
        let cfg = state.config.lock().clone();
        match kind {
            models::ModelKind::Whisper => cfg.whisper_model_path,
            models::ModelKind::Llm => cfg.llm_model_path,
        }
    };
    if !configured.is_empty() && std::path::Path::new(&configured).exists() {
        return Ok(configured);
    }
    let default_path = models::model_path(kind);
    if default_path.exists() {
        return Ok(default_path.to_string_lossy().into_owned());
    }

    let path = models::download(app.clone(), kind)
        .await
        .map_err(|e| e.to_string())?;
    let path_str = path.to_string_lossy().into_owned();
    // Wire the freshly-downloaded model into the user's config so it is
    // picked up by the next `reload_models` call without manual paths.
    let state = app.state::<AppState>();
    let mut cfg = state.config.lock().clone();
    let changed = match kind {
        models::ModelKind::Whisper if cfg.whisper_model_path.is_empty() => {
            cfg.whisper_model_path = path_str.clone();
            true
        }
        models::ModelKind::Llm if cfg.llm_model_path.is_empty() => {
            cfg.llm_model_path = path_str.clone();
            true
        }
        _ => false,
    };
    if changed {
        let _ = config::save(&cfg);
        *state.config.lock() = cfg;
    }
    Ok(path_str)
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    show_window(&app, "settings");
}

#[tauri::command]
fn open_about(app: AppHandle) {
    show_window(&app, "about");
}

#[tauri::command]
fn open_help(app: AppHandle) {
    show_window(&app, "help");
}

fn show_window(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    // Window not declared in tauri.conf.json (e.g. "about"/"help"): create it
    // on demand. The frontend reads `window.location.hash` to pick which view
    // to render (see src/App.tsx).
    let (title, width, height) = match label {
        "about" => ("About OpenWhisper", 480.0, 420.0),
        "help" => ("OpenWhisper Help", 640.0, 640.0),
        "settings" => ("OpenWhisper Settings", 560.0, 720.0),
        _ => ("OpenWhisper", 560.0, 480.0),
    };
    let url = WebviewUrl::App(format!("index.html#{label}").into());
    match WebviewWindowBuilder::new(app, label, url)
        .title(title)
        .inner_size(width, height)
        .resizable(true)
        .build()
    {
        Ok(w) => {
            let _ = w.show();
            let _ = w.set_focus();
        }
        Err(e) => {
            log::error!("failed to create '{label}' window: {e}");
        }
    }
}

/// Version metadata. `latest` is `None` until a successful update check; on
/// success it holds the most recent published GitHub release tag.
#[derive(Clone, Serialize)]
struct VersionInfo {
    current: String,
    latest: Option<String>,
    update_available: bool,
    /// URL of the latest release page, when known. Lets the UI offer a
    /// "Download update" button that opens the browser.
    release_url: Option<String>,
}

/// GitHub repo to query for releases. Override at build time via
/// `OPENWHISPER_RELEASE_REPO=owner/name` if you fork. Falls back to a
/// placeholder which makes the update check no-op gracefully.
const RELEASE_REPO: Option<&str> = option_env!("OPENWHISPER_RELEASE_REPO");

/// Process-lifetime HTTP client. Building a `reqwest::Client` allocates a TLS
/// context and connection pool; sharing one instance avoids recreating them on
/// every update check.
static HTTP_CLIENT: once_cell::sync::Lazy<reqwest::Client> = once_cell::sync::Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("failed to build HTTP client")
});

#[tauri::command]
fn app_version() -> VersionInfo {
    VersionInfo {
        current: env!("CARGO_PKG_VERSION").to_string(),
        latest: None,
        update_available: false,
        release_url: None,
    }
}

#[tauri::command]
async fn check_for_updates() -> Result<VersionInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();

    let Some(repo) = RELEASE_REPO else {
        // No repo wired up at build time — quietly report no update.
        return Ok(VersionInfo {
            current,
            latest: None,
            update_available: false,
            release_url: None,
        });
    };

    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp = HTTP_CLIENT
        .get(&url)
        .header("User-Agent", format!("openwhisper/{current}"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("github request: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("github status: {}", resp.status()));
    }

    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
        html_url: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
    }
    let body = resp
        .text()
        .await
        .map_err(|e| format!("read release body: {e}"))?;
    let release: Release =
        serde_json::from_str(&body).map_err(|e| format!("parse release: {e}"))?;
    if release.draft || release.prerelease {
        return Ok(VersionInfo {
            current,
            latest: None,
            update_available: false,
            release_url: None,
        });
    }

    let latest = release.tag_name.trim_start_matches('v').to_string();
    let update_available = is_newer(&latest, &current);
    Ok(VersionInfo {
        current,
        latest: Some(latest),
        update_available,
        release_url: Some(release.html_url),
    })
}

/// Crude semver comparison: split on '.', compare numerically, fall back to
/// lexicographic for non-numeric parts. Avoids pulling in a full semver crate
/// for what's effectively a 5-line job.
fn is_newer(latest: &str, current: &str) -> bool {
    fn parts(s: &str) -> Vec<u64> {
        s.split(['.', '-', '+'])
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    }
    let a = parts(latest);
    let b = parts(current);
    for i in 0..a.len().max(b.len()) {
        let av = *a.get(i).unwrap_or(&0);
        let bv = *b.get(i).unwrap_or(&0);
        if av != bv {
            return av > bv;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Hotkey parsing & registration
// ---------------------------------------------------------------------------

fn parse_hotkey(s: &str) -> Result<Shortcut> {
    let mut mods = Modifiers::empty();
    let mut key: Option<Code> = None;
    for raw in s.split('+') {
        let part = raw.trim();
        match part.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "meta" | "super" | "win" => mods |= Modifiers::SUPER,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "option" | "opt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "cmdorctrl" => {
                #[cfg(target_os = "macos")]
                {
                    mods |= Modifiers::SUPER;
                }
                #[cfg(not(target_os = "macos"))]
                {
                    mods |= Modifiers::CONTROL;
                }
            }
            other if !other.is_empty() => {
                let code_name = match other {
                    "space" => "Space".to_string(),
                    "enter" | "return" => "Enter".to_string(),
                    "tab" => "Tab".to_string(),
                    "esc" | "escape" => "Escape".to_string(),
                    "fn" | "function" => "Fn".to_string(),
                    s if s.len() == 1 && s.chars().next().unwrap().is_ascii_alphabetic() => {
                        format!("Key{}", s.to_ascii_uppercase())
                    }
                    s if s.len() == 1 && s.chars().next().unwrap().is_ascii_digit() => {
                        format!("Digit{}", s)
                    }
                    other => other.to_string(),
                };
                key = Some(Code::from_str(&code_name).map_err(|_| anyhow!("unknown key: {part}"))?);
            }
            _ => {}
        }
    }
    let code = key.ok_or_else(|| anyhow!("no key in hotkey: {s}"))?;
    Ok(Shortcut::new(Some(mods), code))
}

fn register_hotkey(app: &AppHandle, hotkey_str: &str) -> Result<()> {
    let result = (|| -> Result<Shortcut> {
        let new_shortcut = parse_hotkey(hotkey_str)?;
        let gs = app.global_shortcut();
        let state = app.state::<AppState>();
        if let Some(prev) = state.current_hotkey.lock().take() {
            let _ = gs.unregister(prev);
        }
        gs.register(new_shortcut)?;
        *state.current_hotkey.lock() = Some(new_shortcut);
        Ok(new_shortcut)
    })();

    // Mirror the outcome into engine-status so the Settings UI can surface
    // hotkey conflicts (e.g. another app already owns Cmd+Shift+Space) instead
    // of silently swallowing the error in the log.
    let state = app.state::<AppState>();
    {
        let mut s = state.engine_status.lock();
        s.hotkey = hotkey_str.to_string();
        match &result {
            Ok(_) => {
                s.hotkey_registered = true;
                s.hotkey_error = None;
            }
            Err(e) => {
                s.hotkey_registered = false;
                s.hotkey_error = Some(e.to_string());
            }
        }
    }
    emit_engine_status(app);

    result.map(|_| ())
}

// ---------------------------------------------------------------------------
// Pipeline worker
// ---------------------------------------------------------------------------

fn spawn_pipeline(app: AppHandle) -> Sender<Job> {
    // Bounded at 2: one recording that may already be queued while the
    // pipeline processes another, plus room for a ReloadModels request.
    // Prevents unbounded Vec<f32> accumulation under rapid hotkey spam.
    let (tx, rx) = bounded::<Job>(2);
    let app_handle = app.clone();
    std::thread::spawn(move || {
        // Initial model load.
        load_models(&app_handle);
        while let Ok(job) = rx.recv() {
            match job {
                Job::ReloadModels => load_models(&app_handle),
                Job::Process(samples) => {
                    if let Err(e) = run_pipeline(&app_handle, samples) {
                        emit_status(
                            &app_handle,
                            Status::Error {
                                message: e.to_string(),
                            },
                        );
                        std::thread::sleep(Duration::from_millis(1500));
                        emit_status(&app_handle, Status::Idle);
                    }
                }
            }
        }
    });
    tx
}

fn load_models(app: &AppHandle) {
    let state = app.state::<AppState>();
    let cfg = state.config.lock().clone();
    let inference_threads = config::resolve_inference_threads(cfg.inference_threads);

    let mut whisper_loaded = false;
    let mut whisper_error: Option<String> = None;
    if cfg.whisper_model_path.is_empty() {
        whisper_error = Some("No Whisper model configured. Open Settings → Models.".into());
        emit_log(app, "No Whisper model configured.");
        *state.whisper.lock() = None;
    } else {
        match WhisperEngine::load(
            std::path::Path::new(&cfg.whisper_model_path),
            &cfg.language,
            inference_threads,
        ) {
            Ok(w) => {
                *state.whisper.lock() = Some(Arc::new(w));
                whisper_loaded = true;
                emit_log(
                    app,
                    &format!("Whisper model loaded ({inference_threads} threads)."),
                );
            }
            Err(e) => {
                let msg = format!("{e}");
                emit_log(app, &format!("Failed to load Whisper: {msg}"));
                *state.whisper.lock() = None;
                whisper_error = Some(msg);
            }
        }
    }

    let mut llm_loaded = false;
    let mut llm_error: Option<String> = None;
    if !cfg.use_llm_refinement {
        // Refinement is intentionally disabled — not an error.
        *state.llm.lock() = None;
    } else if cfg.llm_model_path.is_empty() {
        llm_error = Some("No Gemma/LLM model configured. Open Settings → Models.".into());
        *state.llm.lock() = None;
    } else {
        match LlmEngine::load(std::path::Path::new(&cfg.llm_model_path), inference_threads) {
            Ok(l) => {
                *state.llm.lock() = Some(Arc::new(l));
                llm_loaded = true;
                emit_log(
                    app,
                    &format!("LLM model loaded ({inference_threads} threads)."),
                );
            }
            Err(e) => {
                let msg = format!("{e}");
                emit_log(app, &format!("Failed to load LLM: {msg}"));
                *state.llm.lock() = None;
                llm_error = Some(msg);
            }
        }
    }

    {
        let mut s = state.engine_status.lock();
        s.whisper_loaded = whisper_loaded;
        s.whisper_error = whisper_error;
        s.llm_loaded = llm_loaded;
        s.llm_error = llm_error;
    }
    emit_engine_status(app);
}

/// Returns true when the transcription is empty or matches one of the well-known
/// Whisper "silence" hallucinations (e.g. `[BLANK_AUDIO]`, `(silence)`). We use
/// this to skip the refinement+injection steps so the LLM doesn't get prompted
/// with empty input (which causes responses like "Please provide the raw
/// speech-to-text transcription...").
fn is_blank_transcription(raw: &str) -> bool {
    let t = raw.trim();
    if t.is_empty() {
        return true;
    }
    // Strip surrounding bracket/paren wrappers and lowercase for matching.
    let inner = t
        .trim_start_matches(['[', '(', '{', '<'])
        .trim_end_matches([']', ')', '}', '>'])
        .trim()
        .to_ascii_lowercase();
    matches!(
        inner.as_str(),
        "" | "blank_audio"
            | "blank audio"
            | "silence"
            | "silent"
            | "no speech"
            | "no_speech"
            | "inaudible"
            | "music"
            | "background noise"
    )
}

fn run_pipeline(app: &AppHandle, samples: Vec<f32>) -> Result<()> {
    let state = app.state::<AppState>();
    let whisper = state
        .whisper
        .lock()
        .clone()
        .ok_or_else(|| anyhow!("Whisper model not loaded. Open Settings to configure."))?;

    emit_log(
        app,
        &format!(
            "Captured {} samples ({:.2}s @ 16kHz)",
            samples.len(),
            samples.len() as f32 / 16_000.0
        ),
    );

    emit_status(app, Status::Transcribing);
    let raw = whisper.transcribe(&samples)?;
    if raw.is_empty() {
        emit_log(
            app,
            "Whisper returned no text (silence-classified or empty).",
        );
        emit_status(app, Status::Idle);
        return Ok(());
    }
    if is_blank_transcription(&raw) {
        emit_log(
            app,
            &format!("Whisper produced blank marker '{raw}'; skipping."),
        );
        emit_status(app, Status::Idle);
        return Ok(());
    }
    emit_log(app, &format!("Transcribed: {raw}"));

    let cfg = state.config.lock().clone();

    // Decide whether we'll be running the LLM refinement step at all.
    let llm = if cfg.use_llm_refinement {
        state.llm.lock().clone()
    } else {
        None
    };

    // Fast path: if refinement is enabled AND fast_paste is on AND we have an
    // LLM loaded, paste the raw transcript immediately so the user sees text
    // appear with no perceptible delay, then run Gemma in this thread and
    // replace the raw text once the refined version is ready (provided it
    // arrived quickly enough that the user hasn't started typing again).
    if let Some(llm) = llm.clone().filter(|_| cfg.fast_paste) {
        let raw_for_paste = raw.clone();

        // Capture the user's clipboard BEFORE we touch it (only when restore
        // is enabled). We're about to perform two paste operations (raw,
        // then maybe refined) and we want exactly one restore at the very
        // end.
        let original_clipboard = if cfg.restore_clipboard {
            injector::snapshot_clipboard()
        } else {
            None
        };

        emit_status(app, Status::Injecting);
        emit_log(app, &format!("Fast-pasting raw: {raw_for_paste}"));
        let prev_units = match injector::paste_sync(&raw_for_paste) {
            Ok(n) => {
                emit_log(app, &format!("Raw paste OK ({n} units)"));
                n
            }
            Err(e) => {
                emit_log(app, &format!("Raw paste failed: {e}"));
                return Err(e);
            }
        };

        emit_status(app, Status::Refining);
        let refine_start = std::time::Instant::now();
        let refined_result = llm.refine(&cfg.refine_prompt, &raw);
        let elapsed = refine_start.elapsed();

        // If refinement took too long, the user has likely moved on (started
        // typing, switched apps, or just stopped paying attention) and
        // back-spacing through their input would be hostile. Bail out and
        // leave the raw transcript in place.
        const MAX_REPLACE_DELAY: Duration = Duration::from_millis(2000);

        match refined_result {
            Ok(refined) if !refined.trim().is_empty() && refined != raw => {
                if elapsed > MAX_REPLACE_DELAY {
                    emit_log(
                        app,
                        &format!(
                            "Refinement took {}ms (>{}ms); keeping raw to avoid clobbering user input.",
                            elapsed.as_millis(),
                            MAX_REPLACE_DELAY.as_millis()
                        ),
                    );
                } else {
                    emit_log(app, &format!("Replacing with refined: {refined}"));
                    injector::replace_text(prev_units, refined)?;
                }
            }
            Ok(_) => { /* refined is empty or identical — nothing to do. */ }
            Err(e) => emit_log(app, &format!("Refine failed (keeping raw): {e}")),
        }

        // Single restore at the very end, after both pastes have had a chance
        // to land. `restore_clipboard_async` already waits 250 ms.
        if let Some(prev) = original_clipboard {
            injector::restore_clipboard_async(prev);
        }

        std::thread::sleep(Duration::from_millis(150));
        emit_status(app, Status::Idle);
        return Ok(());
    }

    // Slow path: classic refine-then-paste.
    let final_text = if let Some(llm) = llm {
        emit_status(app, Status::Refining);
        match llm.refine(&cfg.refine_prompt, &raw) {
            Ok(refined) if !refined.trim().is_empty() => refined,
            Ok(_) => raw,
            Err(e) => {
                emit_log(app, &format!("Refine failed, using raw: {e}"));
                raw
            }
        }
    } else {
        raw
    };

    if final_text.trim().is_empty() {
        emit_log(app, "Empty final text; skipping injection.");
        emit_status(app, Status::Idle);
        return Ok(());
    }

    emit_status(app, Status::Injecting);
    emit_log(app, &format!("Pasting: {final_text}"));
    injector::type_text(final_text, cfg.restore_clipboard)?;

    std::thread::sleep(Duration::from_millis(150));
    emit_status(app, Status::Idle);
    Ok(())
}

// ---------------------------------------------------------------------------
// Hotkey -> recording state machine
// ---------------------------------------------------------------------------

fn on_hotkey(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutEvent) {
    let state = app.state::<AppState>();
    match event.state() {
        ShortcutState::Pressed => {
            if state.recorder.is_recording() {
                return;
            }
            let max_recording_seconds = state.config.lock().max_recording_seconds;
            if let Err(e) = state.recorder.start(max_recording_seconds) {
                emit_status(
                    app,
                    Status::Error {
                        message: e.to_string(),
                    },
                );
                return;
            }
            start_level_monitor(app.clone(), state.recorder.clone());
        }
        ShortcutState::Released => {
            if !state.recorder.is_recording() {
                return;
            }
            match state.recorder.stop() {
                Ok(samples) => {
                    if let Some(tx) = state.job_tx.lock().clone() {
                        if tx.try_send(Job::Process(samples)).is_err() {
                            log::warn!("pipeline busy; hotkey-triggered recording dropped");
                            emit_status(
                                app,
                                Status::Error {
                                    message: "Pipeline busy; recording dropped.".into(),
                                },
                            );
                        }
                    }
                }
                Err(e) => {
                    emit_status(
                        app,
                        Status::Error {
                            message: e.to_string(),
                        },
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn emit_status(app: &AppHandle, status: Status) {
    update_tray(app, &status);
    let _ = app.emit("status", status);
}

/// Mirror the current status onto the menu-bar tray icon.
///
/// Hot-path optimisations for the Recording state (called every 80 ms):
/// - `set_title` and `set_icon_as_template` are never called here; both are
///   constants (always `None` / `true`) set at tray-build time and never
///   changed elsewhere.
/// - `set_tooltip` is called only on the **first tick** of each recording
///   session; subsequent ticks only update the animated icon frame.
///
/// For non-Recording states, `set_tooltip` is only called when the text
/// actually changes (tracked in `AppState::last_tray_tooltip`).
fn update_tray(app: &AppHandle, status: &Status) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let state = app.state::<AppState>();

    if let Status::Recording { .. } = status {
        let mut f = state.rec_anim_frame.lock();
        let frame = *f;
        *f = f.wrapping_add(1);

        // Update tooltip only once when we first enter the Recording state.
        if frame == 0 {
            let tooltip = "Recording… click to stop";
            *state.last_tray_tooltip.lock() = tooltip;
            let _ = tray.set_tooltip(Some(tooltip));
        }

        let _ = tray.set_icon(tray_icon_recording_frame(frame));
        return;
    }

    // Non-recording state: switch back to the idle template icon and reset
    // the animation frame counter.
    *state.rec_anim_frame.lock() = 0;
    let (icon, tooltip): (Option<Image<'_>>, &'static str) = match status {
        Status::Idle => (tray_icon_template(), "OpenWhisper — click to dictate"),
        Status::Transcribing => (tray_icon_template(), "Transcribing"),
        Status::Refining => (tray_icon_template(), "Refining"),
        Status::Injecting => (tray_icon_template(), "Pasting"),
        Status::Error { .. } => (tray_icon_template(), "Error — see logs"),
        Status::Recording { .. } => unreachable!(),
    };

    {
        let mut last = state.last_tray_tooltip.lock();
        if *last != tooltip {
            *last = tooltip;
            let _ = tray.set_tooltip(Some(tooltip));
        }
    }
    let _ = tray.set_icon(icon);
}

/// Spawn a short-lived thread that emits `Status::Recording` level updates at
/// 80 ms intervals until the recorder stops. Shared by the hotkey handler and
/// the tray-click handler to avoid duplicating the thread-spawn logic.
fn start_level_monitor(app: AppHandle, recorder: Arc<Recorder>) {
    std::thread::spawn(move || {
        while recorder.is_recording() {
            let lvl = *recorder.level.lock();
            emit_status(&app, Status::Recording { level: lvl });
            std::thread::sleep(Duration::from_millis(80));
        }
    });
}

/// Toggle dictation from the tray icon: click once to start, click again to
/// stop and run the pipeline. The transcript is placed on the clipboard and
/// pasted into the focused text field if there is one (see `injector.rs`).
fn toggle_dictation(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.recorder.is_recording() {
        match state.recorder.stop() {
            Ok(samples) => {
                if let Some(tx) = state.job_tx.lock().clone() {
                    if tx.try_send(Job::Process(samples)).is_err() {
                        log::warn!("pipeline busy; tray-triggered recording dropped");
                        emit_status(
                            app,
                            Status::Error {
                                message: "Pipeline busy; recording dropped.".into(),
                            },
                        );
                    }
                }
            }
            Err(e) => emit_status(
                app,
                Status::Error {
                    message: e.to_string(),
                },
            ),
        }
        return;
    }
    let max_recording_seconds = state.config.lock().max_recording_seconds;
    if let Err(e) = state.recorder.start(max_recording_seconds) {
        emit_status(
            app,
            Status::Error {
                message: e.to_string(),
            },
        );
        return;
    }
    start_level_monitor(app.clone(), state.recorder.clone());
}

fn emit_log(app: &AppHandle, msg: &str) {
    log::info!("{msg}");
    let _ = app.emit("log", msg.to_string());
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    on_hotkey(app, shortcut, event);
                })
                .build(),
        )
        .manage(AppState::new())
        .setup(|app| {
            let handle = app.handle().clone();

            // Spawn pipeline thread and stash the sender.
            let tx = spawn_pipeline(handle.clone());
            *handle.state::<AppState>().job_tx.lock() = Some(tx);

            // Register the configured hotkey.
            let hk = handle.state::<AppState>().config.lock().hotkey.clone();
            if let Err(e) = register_hotkey(&handle, &hk) {
                log::error!("failed to register hotkey '{hk}': {e}");
            }

            // System tray: left-click toggles dictation, right-click shows menu.
            use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
            use tauri::tray::TrayIconBuilder;
            let dictate_item =
                MenuItem::with_id(app, "dictate", "Start / Stop Dictation", true, None::<&str>)?;
            let open_item = MenuItem::with_id(app, "open", "Open Settings…", true, None::<&str>)?;
            let help_item = MenuItem::with_id(app, "help", "Help", true, None::<&str>)?;
            let about_item =
                MenuItem::with_id(app, "about", "About OpenWhisper", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &dictate_item,
                    &open_item,
                    &sep1,
                    &about_item,
                    &help_item,
                    &sep2,
                    &quit_item,
                ],
            )?;
            let mut tray_builder = TrayIconBuilder::with_id(TRAY_ID);
            if let Some(icon) = tray_icon_template() {
                tray_builder = tray_builder.icon(icon).icon_as_template(true);
            }
            let _tray = tray_builder
                .menu(&menu)
                // Show the menu on any click of the tray icon. Dictation is
                // still toggleable via the global hotkey or the "Start / Stop
                // Dictation" menu item.
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "dictate" => toggle_dictation(app),
                    "open" => open_settings(app.clone()),
                    "about" => open_about(app.clone()),
                    "help" => open_help(app.clone()),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            emit_status(&handle, Status::Idle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if matches!(window.label(), "settings" | "about" | "help") {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            reload_models,
            pick_file,
            open_settings,
            open_about,
            open_help,
            app_version,
            check_for_updates,
            test_dictate,
            list_models,
            download_model,
            get_engine_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- parse_hotkey ----

    #[test]
    fn parses_simple_hotkey() {
        let hk = parse_hotkey("Fn").expect("valid combo");
        // We can't easily inspect the internal modifiers/code without depending
        // on tauri-plugin-global-shortcut's private API, but the round trip
        // through parse should at least succeed.
        let _ = hk;
    }

    #[test]
    fn accepts_alternate_modifier_names() {
        assert!(parse_hotkey("Control+Alt+A").is_ok());
        assert!(parse_hotkey("ctrl+option+a").is_ok());
        assert!(parse_hotkey("CmdOrCtrl+Shift+Space").is_ok());
        assert!(parse_hotkey("function").is_ok());
    }

    #[test]
    fn rejects_hotkey_with_no_key() {
        assert!(parse_hotkey("Cmd+Shift").is_err());
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(parse_hotkey("Cmd+Shift+Frobnicate").is_err());
    }

    #[test]
    fn parses_letter_and_digit_keys() {
        assert!(parse_hotkey("Ctrl+a").is_ok());
        assert!(parse_hotkey("Ctrl+5").is_ok());
    }

    // ---- is_blank_transcription ----

    #[test]
    fn blank_transcription_detects_empty_and_whitespace() {
        assert!(is_blank_transcription(""));
        assert!(is_blank_transcription("   \n\t"));
    }

    #[test]
    fn blank_transcription_detects_whisper_silence_markers() {
        assert!(is_blank_transcription("[BLANK_AUDIO]"));
        assert!(is_blank_transcription("(silence)"));
        assert!(is_blank_transcription("{music}"));
        assert!(is_blank_transcription("  [No Speech] "));
        assert!(is_blank_transcription("<inaudible>"));
    }

    #[test]
    fn blank_transcription_passes_real_text() {
        assert!(!is_blank_transcription("hello world"));
        assert!(!is_blank_transcription("[hello]")); // bracketed real content
    }

    // ---- is_newer ----

    #[test]
    fn semver_basic_comparisons() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.99"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn semver_handles_v_prefix_already_stripped() {
        // is_newer is called after we strip 'v', so it never sees one. Make
        // sure plain numeric strings work.
        assert!(is_newer("1.2.3", "1.2.2"));
    }

    #[test]
    fn semver_ignores_non_numeric_suffixes() {
        // "1.2.3-rc.1" parses to [1, 2, 3, 1] which is treated as newer than
        // "1.2.3" → [1, 2, 3]. That's a known limitation of our crude
        // comparator and is fine for the update-check use case (we already
        // skip prereleases at the API layer).
        assert!(is_newer("1.2.3-rc.1", "1.2.3"));
    }
}
