//! Audio capture: streams from the default input device into a shared buffer
//! at 16 kHz mono f32 (Whisper's required format).
//!
//! `cpal::Stream` is `!Send + !Sync` on most platforms (it owns a CoreAudio /
//! WASAPI / ALSA handle that must be touched from a single thread). To keep
//! the rest of the app `Send + Sync` we own the stream on a dedicated audio
//! thread and drive it through a command channel.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, Stream, StreamConfig};
use crossbeam_channel::{bounded, Sender};
use parking_lot::Mutex;
use std::sync::Arc;

const TARGET_SR: u32 = 16_000;

enum Cmd {
    Start {
        reply: crossbeam_channel::Sender<Result<()>>,
        max_seconds: u32,
    },
    Stop(crossbeam_channel::Sender<Result<Vec<f32>>>),
}

pub struct Recorder {
    cmd_tx: Sender<Cmd>,
    recording: Arc<Mutex<bool>>,
    pub level: Arc<Mutex<f32>>,
}

impl Recorder {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = bounded::<Cmd>(8);
        let recording = Arc::new(Mutex::new(false));
        let level = Arc::new(Mutex::new(0.0f32));

        let recording_thread = recording.clone();
        let level_thread = level.clone();
        std::thread::spawn(move || audio_thread(cmd_rx, recording_thread, level_thread));

        Self {
            cmd_tx,
            recording,
            level,
        }
    }

    pub fn is_recording(&self) -> bool {
        *self.recording.lock()
    }

    pub fn start(&self, max_seconds: u32) -> Result<()> {
        if self.is_recording() {
            return Ok(());
        }
        let (tx, rx) = bounded(1);
        self.cmd_tx
            .send(Cmd::Start {
                reply: tx,
                max_seconds,
            })
            .map_err(|_| anyhow!("audio thread dead"))?;
        rx.recv()
            .map_err(|_| anyhow!("audio thread dropped reply"))?
    }

    pub fn stop(&self) -> Result<Vec<f32>> {
        if !self.is_recording() {
            return Err(anyhow!("not recording"));
        }
        let (tx, rx) = bounded(1);
        self.cmd_tx
            .send(Cmd::Stop(tx))
            .map_err(|_| anyhow!("audio thread dead"))?;
        rx.recv()
            .map_err(|_| anyhow!("audio thread dropped reply"))?
    }
}

struct ActiveStream {
    _stream: Stream,
    /// Mono samples at the source sample rate. We downmix in the audio
    /// callback so the stored buffer is always 1 channel of f32 — this cuts
    /// memory by 2× on stereo mics and removes the channel-stride bookkeeping
    /// from the stop path. (Resampling to 16 kHz still happens on stop.)
    mono_samples: Arc<Mutex<Vec<f32>>>,
    src_sr: u32,
}

fn audio_thread(
    cmd_rx: crossbeam_channel::Receiver<Cmd>,
    recording: Arc<Mutex<bool>>,
    level: Arc<Mutex<f32>>,
) {
    let mut active: Option<ActiveStream> = None;

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            Cmd::Start { reply, max_seconds } => {
                if active.is_some() {
                    let _ = reply.send(Ok(()));
                    continue;
                }
                match build_stream(&level, max_seconds) {
                    Ok(a) => {
                        active = Some(a);
                        *recording.lock() = true;
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }
            Cmd::Stop(reply) => {
                if let Some(a) = active.take() {
                    *recording.lock() = false;
                    *level.lock() = 0.0;
                    let ActiveStream {
                        mono_samples,
                        src_sr,
                        ..
                    } = a;
                    let mono = std::mem::take(&mut *mono_samples.lock());
                    let resampled = resample_linear(&mono, src_sr, TARGET_SR);
                    let _ = reply.send(Ok(resampled));
                } else {
                    let _ = reply.send(Err(anyhow!("not recording")));
                }
            }
        }
    }
}

fn build_stream(level: &Arc<Mutex<f32>>, max_seconds: u32) -> Result<ActiveStream> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("no default input device"))?;
    let config = device
        .default_input_config()
        .context("default_input_config failed")?;

    let src_sr = config.sample_rate().0;
    let src_channels = config.channels();
    let sample_format = config.sample_format();
    let stream_config: StreamConfig = config.into();

    let max_seconds = max_seconds.max(1);
    let max_frames = (src_sr as usize).saturating_mul(max_seconds as usize);

    // Pre-size for ~4 seconds of mono source-rate audio, capped by the
    // configured recording limit so accidental long recordings stay bounded.
    let mono_samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(
        max_frames.min((src_sr as usize) * 4),
    )));

    let err_fn = |e| log::error!("audio stream error: {e}");

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            {
                let samples = mono_samples.clone();
                let level = level.clone();
                move |data: &[f32], _| {
                    push_interleaved(&samples, &level, data, src_channels, max_frames)
                }
            },
            err_fn,
            None,
        )?,
        SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            {
                let samples = mono_samples.clone();
                let level = level.clone();
                move |data: &[i16], _| {
                    // Convert i16 → f32 inline; avoid allocating a temporary Vec.
                    push_interleaved_iter(
                        &samples,
                        &level,
                        data.iter().map(|s| s.to_sample::<f32>()),
                        data.len(),
                        src_channels,
                        max_frames,
                    );
                }
            },
            err_fn,
            None,
        )?,
        SampleFormat::U16 => device.build_input_stream(
            &stream_config,
            {
                let samples = mono_samples.clone();
                let level = level.clone();
                move |data: &[u16], _| {
                    push_interleaved_iter(
                        &samples,
                        &level,
                        data.iter().map(|s| s.to_sample::<f32>()),
                        data.len(),
                        src_channels,
                        max_frames,
                    );
                }
            },
            err_fn,
            None,
        )?,
        other => return Err(anyhow!("unsupported sample format: {other:?}")),
    };

    stream.play()?;
    Ok(ActiveStream {
        _stream: stream,
        mono_samples,
        src_sr,
    })
}

/// Hot-path push for f32 inputs. Downmixes to mono and updates the level
/// meter without any heap allocation.
fn push_interleaved(
    samples: &Mutex<Vec<f32>>,
    level: &Mutex<f32>,
    data: &[f32],
    channels: u16,
    max_frames: usize,
) {
    if data.is_empty() {
        return;
    }
    let c = channels.max(1) as usize;
    let frames = data.len() / c;
    let inv_c = 1.0 / c as f32;

    // Compute level over the mono-downmixed values to keep the meter
    // consistent regardless of channel count.
    let mut sum_sq = 0.0f32;
    let mut buf = samples.lock();
    let frames_to_keep = frames.min(max_frames.saturating_sub(buf.len()));
    if frames_to_keep == 0 {
        return;
    }
    buf.reserve(frames_to_keep);
    for i in 0..frames_to_keep {
        let mut acc = 0.0f32;
        for ch in 0..c {
            acc += data[i * c + ch];
        }
        let mono = acc * inv_c;
        sum_sq += mono * mono;
        buf.push(mono);
    }
    drop(buf);
    let rms = (sum_sq / frames_to_keep.max(1) as f32).sqrt();
    *level.lock() = (rms * 4.0).clamp(0.0, 1.0);
}

/// Generic variant for non-f32 sample formats. Takes an iterator yielding
/// already-converted f32 samples so we don't need an intermediate `Vec`.
fn push_interleaved_iter<I: Iterator<Item = f32>>(
    samples: &Mutex<Vec<f32>>,
    level: &Mutex<f32>,
    iter: I,
    n_samples: usize,
    channels: u16,
    max_frames: usize,
) {
    if n_samples == 0 {
        return;
    }
    let c = channels.max(1) as usize;
    let frames = n_samples / c;
    let inv_c = 1.0 / c as f32;

    let mut sum_sq = 0.0f32;
    let mut buf = samples.lock();
    let frames_to_keep = frames.min(max_frames.saturating_sub(buf.len()));
    if frames_to_keep == 0 {
        return;
    }
    buf.reserve(frames_to_keep);

    // Walk the iterator a frame at a time using a fixed-size scratch.
    let mut iter = iter;
    for _ in 0..frames_to_keep {
        let mut acc = 0.0f32;
        for _ in 0..c {
            acc += iter.next().unwrap_or(0.0);
        }
        let mono = acc * inv_c;
        sum_sq += mono * mono;
        buf.push(mono);
    }
    drop(buf);
    let rms = (sum_sq / frames_to_keep.max(1) as f32).sqrt();
    *level.lock() = (rms * 4.0).clamp(0.0, 1.0);
}

/// Resample `input` from `src_sr` to `dst_sr`. When downsampling we apply a
/// windowed-sinc anti-alias filter at the new Nyquist, then linearly
/// interpolate at the target sample positions.
///
/// Unlike the previous two-pass approach (lowpass_sinc → full filtered Vec →
/// lerp), this single-pass implementation evaluates the FIR kernel on-demand
/// at each output position, eliminating the intermediate allocation. For a
/// 30 s clip at 48 kHz that halves peak RSS from ~12 MB to ~7 MB during the
/// stop/resample step (source buffer + output only; no full filtered copy).
fn resample_linear(input: &[f32], src_sr: u32, dst_sr: u32) -> Vec<f32> {
    if src_sr == dst_sr || input.is_empty() {
        return input.to_vec();
    }

    let ratio = dst_sr as f64 / src_sr as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    let last = input.len().saturating_sub(1);

    if dst_sr < src_sr {
        // Build the anti-alias kernel once, then evaluate it per output
        // position without materialising the full filtered intermediate buffer.
        let fc = (0.45 * dst_sr as f32) / src_sr as f32;
        let kernel = build_lowpass_kernel(fc, 64);
        let half = (kernel.len() - 1) / 2;

        for i in 0..out_len {
            let src_pos = i as f64 / ratio;
            let i0 = (src_pos.floor() as usize).min(last);
            let frac = (src_pos - i0 as f64) as f32;
            let v0 = apply_fir_at(input, i0, &kernel, half);
            if frac < 1e-6 {
                out.push(v0);
            } else {
                let v1 = apply_fir_at(input, (i0 + 1).min(last), &kernel, half);
                out.push(v0 * (1.0 - frac) + v1 * frac);
            }
        }
    } else {
        // Upsampling: no anti-aliasing needed (new Nyquist is higher).
        for i in 0..out_len {
            let src_pos = i as f64 / ratio;
            let i0 = (src_pos.floor() as usize).min(last);
            let i1 = (i0 + 1).min(last);
            let frac = (src_pos - i0 as f64) as f32;
            out.push(input[i0] * (1.0 - frac) + input[i1] * frac);
        }
    }
    out
}

/// Build a Hann-windowed sinc low-pass kernel. `fc` is the normalised cutoff
/// frequency (`cutoff_hz / sample_rate`, range 0..0.5). `taps` is forced even.
fn build_lowpass_kernel(fc: f32, taps: usize) -> Vec<f32> {
    let taps = taps.max(8) & !1; // force even
    let half = taps / 2;
    let mut kernel = vec![0.0f32; taps + 1];
    let mut sum = 0.0f32;
    for i in 0..=taps {
        let n = i as f32 - half as f32;
        let sinc = if n.abs() < 1e-6 {
            2.0 * fc
        } else {
            (2.0 * std::f32::consts::PI * fc * n).sin() / (std::f32::consts::PI * n)
        };
        let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / taps as f32).cos());
        kernel[i] = sinc * w;
        sum += kernel[i];
    }
    if sum.abs() > 1e-9 {
        for k in &mut kernel {
            *k /= sum;
        }
    }
    kernel
}

/// Evaluate the FIR `kernel` centred at `center` within `input`. Out-of-bounds
/// positions are treated as zero (Dirichlet boundary).
fn apply_fir_at(input: &[f32], center: usize, kernel: &[f32], half: usize) -> f32 {
    let mut acc = 0.0f32;
    let n = input.len();
    for (k, &coeff) in kernel.iter().enumerate() {
        let src = center as isize + k as isize - half as isize;
        if src >= 0 && (src as usize) < n {
            acc += input[src as usize] * coeff;
        }
    }
    acc
}

/// Symmetric FIR low-pass via windowed sinc. `taps` must be even.
/// Compiled only in test builds; production audio goes through `resample_linear`.
#[cfg(test)]
fn lowpass_sinc(input: &[f32], sample_rate: f32, cutoff_hz: f32, taps: usize) -> Vec<f32> {
    let taps = taps.max(8) & !1;
    let fc = cutoff_hz / sample_rate;
    let kernel = build_lowpass_kernel(fc, taps);
    let half = (kernel.len() - 1) / 2;
    (0..input.len())
        .map(|i| apply_fir_at(input, i, &kernel, half))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn resample_passthrough_when_rates_match() {
        let input: Vec<f32> = (0..100).map(|i| (i as f32 * 0.01).sin()).collect();
        let out = resample_linear(&input, 16_000, 16_000);
        assert_eq!(out, input);
    }

    #[test]
    fn resample_changes_length_proportionally() {
        // 480 samples @ 48kHz = 10ms → expect ~160 samples @ 16kHz.
        let input = vec![0.1f32; 480];
        let out = resample_linear(&input, 48_000, 16_000);
        let ratio = out.len() as f32 / input.len() as f32;
        assert!(
            (ratio - 1.0 / 3.0).abs() < 0.05,
            "expected ~1/3 ratio, got {ratio}"
        );
    }

    #[test]
    fn lowpass_attenuates_above_cutoff() {
        // Build a signal with two tones: 500 Hz (well below cutoff) and
        // 18 kHz (well above the 7.2 kHz cutoff for the 16 kHz target
        // Nyquist filter at sample rate 48 kHz, and outside even a wide
        // transition band). Verify the high tone is heavily attenuated.
        let sr = 48_000.0f32;
        let n = 4096;
        let low_hz = 500.0f32;
        let high_hz = 18_000.0f32;
        let mut sig = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / sr;
            sig.push((2.0 * PI * low_hz * t).sin() + (2.0 * PI * high_hz * t).sin());
        }
        // Filter with cutoff at 0.45 * 16 kHz target (matches resample_linear).
        let filtered = lowpass_sinc(&sig, sr, 0.45 * 16_000.0, 64);

        // Compare against a low-only reference signal so we can isolate the
        // high-frequency component's energy.
        let low_only: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                (2.0 * PI * low_hz * t).sin()
            })
            .collect();

        // Steady-state region, past kernel transients on both ends.
        let lo = 256;
        let hi = n - 256;
        let raw_energy: f32 = sig[lo..hi].iter().map(|x| x * x).sum();
        let filt_energy: f32 = filtered[lo..hi].iter().map(|x| x * x).sum();
        let low_energy: f32 = low_only[lo..hi].iter().map(|x| x * x).sum();

        // Filtered signal should be much closer to "low tone only" than to the
        // raw two-tone input.
        assert!(
            filt_energy < raw_energy * 0.65,
            "low-pass didn't attenuate enough: raw={raw_energy:.2}, filtered={filt_energy:.2}"
        );
        assert!(
            (filt_energy - low_energy).abs() < low_energy * 0.25,
            "filtered energy {filt_energy:.2} should be near low-only {low_energy:.2}"
        );
    }

    #[test]
    fn lowpass_preserves_dc() {
        let sig = vec![1.0f32; 1024];
        let out = lowpass_sinc(&sig, 48_000.0, 7_200.0, 64);
        // DC gain should be very close to 1 in the middle of the buffer
        // (edges are tapered by the truncation-as-zero boundary).
        let mid = out[out.len() / 2];
        assert!((mid - 1.0).abs() < 0.05, "DC gain off: {mid}");
    }
}
