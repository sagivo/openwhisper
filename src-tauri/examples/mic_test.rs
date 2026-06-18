//! Records `seconds` seconds from the default mic via the same Recorder
//! used in the app, then runs Whisper on it. Proves cpal capture works.
//!
//! Usage:
//!   cargo run --example mic_test --features metal -- <whisper.bin> [seconds]

use anyhow::{Context, Result};
use openwhisper_lib::audio::Recorder;
use openwhisper_lib::config::{default_max_recording_seconds, resolve_inference_threads};
use openwhisper_lib::whisper_engine::WhisperEngine;
use std::path::Path;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let whisper_path = args.next().context("usage: <whisper.bin> [seconds]")?;
    let seconds: u64 = args
        .next()
        .as_deref()
        .unwrap_or("3")
        .parse()
        .context("seconds must be u64")?;

    println!("loading whisper...");
    let n_threads = resolve_inference_threads(0);
    let whisper = WhisperEngine::load(Path::new(&whisper_path), "en", n_threads)?;

    let recorder = Recorder::new();
    println!("recording {seconds} seconds (speak now)...");
    recorder
        .start(default_max_recording_seconds().max(seconds as u32))
        .context("start recording")?;
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(seconds) {
        std::thread::sleep(Duration::from_millis(200));
        let lvl = *recorder.level.lock();
        let bars = "█".repeat((lvl * 30.0) as usize);
        eprint!("\r  level: [{bars:<30}] {lvl:.2}");
    }
    eprintln!();

    let samples = recorder.stop().context("stop")?;
    println!(
        "captured {} samples ({:.2}s of 16kHz audio)",
        samples.len(),
        samples.len() as f32 / 16_000.0
    );
    if samples.is_empty() {
        anyhow::bail!("captured zero samples — mic permission may be missing");
    }

    println!("transcribing...");
    let raw = whisper.transcribe(&samples)?;
    println!("RAW: {raw:?}");
    Ok(())
}
