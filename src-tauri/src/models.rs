//! First-run model download support.
//!
//! Downloads default Whisper + Gemma models into the user's data directory
//! (`~/Library/Application Support/openwhisper/models` on macOS) and reports
//! progress to the frontend via Tauri events.

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Whisper,
    Llm,
}

impl ModelKind {
    fn url(self) -> &'static str {
        match self {
            ModelKind::Whisper => {
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"
            }
            // Gemma 4 E4B Instruct, Q4_K_M (~4.98 GB). The unsloth mirror is
            // anonymous-downloadable (the official `google/gemma-4-E4B-it`
            // and `ggml-org/gemma-4-E4B-it-GGUF` repos are gated and require
            // an HF auth token).
            ModelKind::Llm => {
                "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf"
            }
        }
    }

    fn filename(self) -> &'static str {
        match self {
            ModelKind::Whisper => "ggml-base.en.bin",
            ModelKind::Llm => "gemma-4-E4B-it-Q4_K_M.gguf",
        }
    }

    fn key(self) -> &'static str {
        match self {
            ModelKind::Whisper => "whisper",
            ModelKind::Llm => "llm",
        }
    }
}

pub fn models_dir() -> PathBuf {
    let dir = dirs::data_dir()
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("openwhisper")
        .join("models");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn model_path(kind: ModelKind) -> PathBuf {
    models_dir().join(kind.filename())
}

#[derive(Serialize)]
pub struct ModelStatus {
    pub key: &'static str,
    pub filename: &'static str,
    pub path: String,
    pub exists: bool,
    pub size: u64,
}

pub fn status(kind: ModelKind) -> ModelStatus {
    status_with_config(kind, "")
}

/// Like [`status`], but also treats the user's configured model path as
/// "installed" when it points to an existing file. This avoids prompting the
/// user to re-download a model they have already supplied via Browse… or by
/// dropping a file onto the default location with a different filename.
pub fn status_with_config(kind: ModelKind, configured_path: &str) -> ModelStatus {
    let default_path = model_path(kind);
    let (path, exists, size) = if !configured_path.is_empty() {
        match std::fs::metadata(configured_path) {
            Ok(m) => (PathBuf::from(configured_path), true, m.len()),
            Err(_) => match std::fs::metadata(&default_path) {
                Ok(m) => (default_path, true, m.len()),
                Err(_) => (PathBuf::from(configured_path), false, 0),
            },
        }
    } else {
        match std::fs::metadata(&default_path) {
            Ok(m) => (default_path, true, m.len()),
            Err(_) => (default_path, false, 0),
        }
    };
    ModelStatus {
        key: kind.key(),
        filename: kind.filename(),
        path: path.to_string_lossy().into_owned(),
        exists,
        size,
    }
}

#[derive(Clone, Serialize)]
struct ProgressEvent<'a> {
    key: &'a str,
    downloaded: u64,
    total: u64,
    done: bool,
}

pub async fn download(app: AppHandle, kind: ModelKind) -> Result<PathBuf> {
    let dest = model_path(kind);
    if dest.exists() {
        return Ok(dest);
    }
    let tmp = dest.with_extension("part");
    if tmp.exists() {
        let _ = tokio::fs::remove_file(&tmp).await;
    }

    let client = reqwest::Client::builder()
        .user_agent("openwhisper/0.1")
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .build()?;

    let resp = client
        .get(kind.url())
        .send()
        .await
        .with_context(|| format!("requesting {}", kind.url()))?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "download failed for {}: HTTP {}",
            kind.filename(),
            resp.status()
        ));
    }
    let total = resp.content_length().unwrap_or(0);

    if let Some(parent) = tmp.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .with_context(|| format!("creating {}", tmp.display()))?;

    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = tokio::time::timeout(DOWNLOAD_IDLE_TIMEOUT, stream.next())
        .await
        .with_context(|| format!("download stalled for {}", kind.filename()))?
    {
        let chunk = chunk.context("reading chunk")?;
        file.write_all(&chunk).await.context("writing chunk")?;
        downloaded += chunk.len() as u64;
        if downloaded - last_emit > 256 * 1024 || downloaded == total {
            last_emit = downloaded;
            let _ = app.emit(
                "model-progress",
                ProgressEvent {
                    key: kind.key(),
                    downloaded,
                    total,
                    done: false,
                },
            );
        }
    }
    file.flush().await.context("flushing model file")?;
    file.sync_all().await.context("syncing model file")?;
    drop(file);

    tokio::fs::rename(&tmp, &dest)
        .await
        .with_context(|| format!("renaming {} -> {}", tmp.display(), dest.display()))?;
    if let Some(parent) = dest.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    let _ = app.emit(
        "model-progress",
        ProgressEvent {
            key: kind.key(),
            downloaded,
            total,
            done: true,
        },
    );
    Ok(dest)
}
