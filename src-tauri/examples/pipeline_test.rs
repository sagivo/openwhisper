//! End-to-end pipeline test: WAV file -> Whisper -> Gemma refinement.
//!
//! Usage:
//!   cargo run --example pipeline_test --features metal -- \
//!       ../models/ggml-base.en.bin \
//!       ../models/gemma-2-2b-it-Q4_K_M.gguf \
//!       ../test-data/jfk.wav

use anyhow::{Context, Result};
use openwhisper_lib::config::{resolve_inference_threads, DEFAULT_REFINE_PROMPT};
use openwhisper_lib::llm_engine::LlmEngine;
use openwhisper_lib::whisper_engine::WhisperEngine;
use std::path::Path;
use std::time::Instant;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let whisper_path = args
        .next()
        .context("usage: <whisper.bin> <gemma.gguf> <audio.wav>")?;
    let llm_path = args
        .next()
        .context("usage: <whisper.bin> <gemma.gguf> <audio.wav>")?;
    let wav_path = args
        .next()
        .context("usage: <whisper.bin> <gemma.gguf> <audio.wav>")?;
    let n_threads = resolve_inference_threads(0);

    println!("== loading whisper ==");
    let t0 = Instant::now();
    let whisper = WhisperEngine::load(Path::new(&whisper_path), "en", n_threads)?;
    println!("  loaded in {:.2}s", t0.elapsed().as_secs_f32());

    println!("== loading gemma ==");
    let t0 = Instant::now();
    let llm = LlmEngine::load(Path::new(&llm_path), n_threads)?;
    println!("  loaded in {:.2}s", t0.elapsed().as_secs_f32());

    println!("== reading wav ==");
    let samples = read_wav_16k_mono(&wav_path)?;
    println!(
        "  {} samples ({:.2}s of audio at 16kHz)",
        samples.len(),
        samples.len() as f32 / 16000.0
    );

    println!("== transcribing ==");
    let t0 = Instant::now();
    let raw = whisper.transcribe(&samples)?;
    println!("  transcribed in {:.2}s", t0.elapsed().as_secs_f32());
    println!("  RAW: {raw:?}");
    assert!(!raw.is_empty(), "whisper produced empty output");

    println!("== refining ==");
    let t0 = Instant::now();
    let refined = llm.refine(DEFAULT_REFINE_PROMPT, &raw)?;
    println!("  refined in {:.2}s", t0.elapsed().as_secs_f32());
    println!("  REFINED: {refined:?}");
    assert!(!refined.is_empty(), "gemma produced empty output");

    println!("\n== second pass (warm) ==");
    let t0 = Instant::now();
    let raw2 = whisper.transcribe(&samples)?;
    println!("  whisper warm: {:.2}s", t0.elapsed().as_secs_f32());
    let t0 = Instant::now();
    let refined2 = llm.refine(DEFAULT_REFINE_PROMPT, &raw2)?;
    println!("  gemma warm:   {:.2}s", t0.elapsed().as_secs_f32());
    println!("  REFINED2: {refined2:?}");

    println!("\nOK");
    Ok(())
}

fn read_wav_16k_mono(path: &str) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path).context("open wav")?;
    let spec = reader.spec();
    println!("  wav spec: {:?}", spec);

    let raw_f32: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => match spec.bits_per_sample {
            16 => reader
                .samples::<i16>()
                .map(|s| s.unwrap_or(0) as f32 / i16::MAX as f32)
                .collect(),
            32 => reader
                .samples::<i32>()
                .map(|s| s.unwrap_or(0) as f32 / i32::MAX as f32)
                .collect(),
            n => anyhow::bail!("unsupported bit depth: {n}"),
        },
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
    };

    let channels = spec.channels as usize;
    let mono: Vec<f32> = if channels == 1 {
        raw_f32
    } else {
        raw_f32
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    let resampled = if spec.sample_rate == 16_000 {
        mono
    } else {
        let ratio = 16_000.0 / spec.sample_rate as f64;
        let out_len = ((mono.len() as f64) * ratio).round() as usize;
        (0..out_len)
            .map(|i| {
                let pos = i as f64 / ratio;
                let i0 = pos.floor() as usize;
                let i1 = (i0 + 1).min(mono.len() - 1);
                let frac = (pos - i0 as f64) as f32;
                mono[i0] * (1.0 - frac) + mono[i1] * frac
            })
            .collect()
    };

    Ok(resampled)
}
