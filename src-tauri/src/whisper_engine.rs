use anyhow::{anyhow, Context, Result};
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct WhisperEngine {
    /// Whisper.cpp's context is internally reference-counted and exposes
    /// `create_state(&self)` as a non-mutating call, so we don't need a
    /// Mutex around it. Each transcription gets its own fresh `WhisperState`,
    /// which is the part that actually carries the per-utterance scratch.
    ctx: WhisperContext,
    language: String,
    n_threads: i32,
}

impl WhisperEngine {
    pub fn load(model_path: &Path, language: &str, n_threads: i32) -> Result<Self> {
        if !model_path.exists() {
            return Err(anyhow!("whisper model not found: {}", model_path.display()));
        }
        let ctx_params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .ok_or_else(|| anyhow!("non-utf8 model path"))?,
            ctx_params,
        )
        .context("loading whisper context")?;
        Ok(Self {
            ctx,
            language: language.to_string(),
            n_threads: n_threads.max(1),
        })
    }

    pub fn transcribe(&self, samples: &[f32]) -> Result<String> {
        if samples.len() < 1600 {
            // < 0.1s of audio at 16kHz, skip.
            return Ok(String::new());
        }

        // Only use the VAD as a *reject*, not as a *trim*: feed Whisper the
        // whole clip and let it figure out the boundaries. We reject only when
        // the entire clip is well below any plausible voice level, matching
        // the previous global-RMS behavior but with a noise-floor-aware
        // estimate so quiet voices in noisy mics still get through.
        if is_all_silence(samples) {
            log::info!("audio is all silence, skipping");
            return Ok(String::new());
        }

        let mut state = self.ctx.create_state().context("create whisper state")?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let lang = if self.language.is_empty() || self.language == "auto" {
            None
        } else {
            Some(self.language.as_str())
        };
        params.set_language(lang);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_translate(false);
        params.set_no_context(true);
        params.set_single_segment(false);
        params.set_suppress_blank(true);
        params.set_n_threads(self.n_threads);

        state
            .full(params, samples)
            .context("whisper full() failed")?;

        let mut out = String::new();
        for segment in state.as_iter() {
            out.push_str(&segment.to_string());
        }
        Ok(out.trim().to_string())
    }
}

/// Returns true when the recording looks like pure silence — i.e. its peak
/// frame energy is below a permissive threshold. We deliberately do NOT
/// trim: Whisper's own segmenter handles silence trimming internally far
/// better than a per-frame energy gate, and over-aggressive trimming is what
/// caused the regression where legitimate dictation produced no output.
///
/// The check is against the *peak* (95th percentile) frame energy, not the
/// global RMS, so a 5-second clip with one 1-second utterance is correctly
/// classified as "has speech" even though most frames are quiet. A clip
/// where even the loudest frames are below 0.005 RMS is considered silence.
fn is_all_silence(samples: &[f32]) -> bool {
    const SR: usize = 16_000;
    const FRAME: usize = SR * 30 / 1000; // 480 samples = 30 ms
    /// Permissive absolute threshold. A whisper or quiet mic typically peaks
    /// well above this — only true silence (or muted mic) stays below.
    const PEAK_THRESHOLD: f32 = 0.005;

    if samples.len() < FRAME * 2 {
        return false;
    }

    let n_frames = samples.len() / FRAME;
    let mut energies: Vec<f32> = Vec::with_capacity(n_frames);
    for f in 0..n_frames {
        let frame = &samples[f * FRAME..(f + 1) * FRAME];
        let mut sum = 0.0f32;
        for &s in frame {
            sum += s * s;
        }
        energies.push((sum / FRAME as f32).sqrt());
    }

    // 95th percentile of frame energies — robust to a single outlier click
    // or pop, while still reflecting the loudest sustained moments.
    let idx = (((energies.len() as f32) * 0.95) as usize).min(energies.len() - 1);
    energies.select_nth_unstable_by(idx, |a, b| {
        a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
    });
    let peak = energies[idx];

    peak < PEAK_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silence(seconds: f32) -> Vec<f32> {
        vec![0.0f32; (16_000.0 * seconds) as usize]
    }

    fn tone(seconds: f32, amplitude: f32) -> Vec<f32> {
        let n = (16_000.0 * seconds) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / 16_000.0;
            out.push(amplitude * (2.0 * std::f32::consts::PI * 440.0 * t).sin());
        }
        out
    }

    /// Quiet hiss only — well below the 0.005 floor.
    fn hiss(seconds: f32) -> Vec<f32> {
        let n = (16_000.0 * seconds) as usize;
        let mut out = Vec::with_capacity(n);
        let mut seed: u32 = 0xdeadbeef;
        for _ in 0..n {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            out.push((seed as f32 / u32::MAX as f32 - 0.5) * 0.0005);
        }
        out
    }

    #[test]
    fn pure_silence_is_classified_as_silence() {
        assert!(is_all_silence(&silence(2.0)));
    }

    #[test]
    fn quiet_hiss_is_classified_as_silence() {
        assert!(is_all_silence(&hiss(2.0)));
    }

    #[test]
    fn loud_speech_is_not_silence() {
        assert!(!is_all_silence(&tone(1.0, 0.3)));
    }

    #[test]
    fn brief_speech_in_long_silence_is_not_silence() {
        // 4 s silence + 1 s speech + 4 s silence: a real recording where the
        // user paused before/after speaking. Must pass through to Whisper.
        let mut clip = silence(4.0);
        clip.extend(tone(1.0, 0.3));
        clip.extend(silence(4.0));
        assert!(!is_all_silence(&clip));
    }

    #[test]
    fn very_short_clips_are_not_silence() {
        // Too short to evaluate; let Whisper handle it.
        let s = vec![0.5f32; 100];
        assert!(!is_all_silence(&s));
    }
}
