# Default local STT model

**Question:** which local speech-to-text model should OpenWhisper ship as the default going forward — small enough for on-device use, fast enough for click-to-talk dictation, good enough that Gemma is cleaning language rather than repairing mishears.

**Date:** 2026-08-29

**Shipped default (regular laptops):** `ggml-small.en.bin` (~466 MiB). Turbo keeps the large-v3 encoder, so short clips on CPU-only 16 GB machines (already holding Gemma ~5 GB) can feel slower than `small.en`. Keep turbo-q5 as an optional Metal/quality choice, not the first-run download.

**Earlier recommendation in this note** (Apple Silicon / quality-first) was **`ggml-large-v3-turbo-q5_0.bin`**. Drop-in for the current `whisper-rs` / whisper.cpp stack. ~547 MiB on disk. Do not change the inference engine for this.

---

## Constraints from this repo

OpenWhisper is a menu-bar dictation app: short clips (seconds, not hours), 16 kHz mono, greedy Whisper decode, then a local Gemma 4 E4B refiner. The STT path is `whisper-rs` 0.16 wrapping whisper.cpp. Models load once at startup and stay resident next to Gemma.

Current default is English-only Whisper **base.en**:

| | |
| --- | --- |
| File | `ggml-base.en.bin` |
| URL | `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin` |
| Disk | 147,964,211 bytes (~141 MiB); whisper.cpp lists 142 MiB |
| Runtime mem (whisper.cpp table) | ~388 MB for `base` |

Wired in `scripts/download-models.sh`, `src-tauri/src/models.rs`, and `src-tauri/src/config.rs`.

Gemma 4 E4B Instruct Q4_K_M is ~5.0 GB. Any STT default has to coexist with that, not compete with it. Release builds on macOS enable Metal (`npm run release`).

The file picker already accepts `.bin` / `.ggml`, so a quantized turbo `.bin` needs no format change.

---

## What “good and fast” means here

Dictation is **short-form ASR** (typically well under Whisper’s 30 s window). Quality that matters:

- Word error rate on English, including noisy mics and accents
- Fewer hallucinated sentences (`Thanks for watching!` is already called out in the README)
- Speed to first transcript, because fast-paste shows the raw Whisper text immediately
- Disk and RAM that leave room for Gemma on a 16 GB Mac

OpenAI’s own relative-speed table (English speech on an A100, relative to `large` = 1×):

| Size | Parameters | English-only | VRAM | Relative speed |
| --- | --- | --- | --- | --- |
| tiny | 39 M | `tiny.en` | ~1 GB | ~10× |
| base | 74 M | `base.en` | ~1 GB | ~7× |
| small | 244 M | `small.en` | ~2 GB | ~4× |
| medium | 769 M | `medium.en` | ~5 GB | ~2× |
| large | 1550 M | — | ~10 GB | 1× |
| **turbo** | **809 M** | — | **~6 GB** | **~8×** |

Source: [openai/whisper README, “Available models and languages”](https://github.com/openai/whisper/blob/main/README.md). The model card lists turbo at 798 M parameters; the README table says 809 M. Same model either way.

OpenAI also notes that `.en` models “tend to perform better, especially for the `tiny.en` and `base.en` models,” and that “the difference becomes less significant for the `small.en` and `medium.en` models.” Turbo has no `.en` variant because it is a pruned `large-v3`.

Since September 2024, the `whisper` CLI **defaults to `turbo`**.

---

## Candidates that stay on whisper.cpp

These are the only models that can become the default without rewriting `whisper_engine.rs`. Sizes below are `Content-Length` from Hugging Face `ggerganov/whisper.cpp` (HEAD, 2026-08-29), plus the official whisper.cpp disk/mem table where it exists.

| Model | Disk (bytes / listed) | Runtime mem (whisper.cpp) | Why consider it |
| --- | --- | --- | --- |
| `tiny.en` | 77,704,715 (~75 MiB) | ~273 MB | Fastest Whisper. Too lossy for dictation that a user will paste. |
| `tiny.en-q5_1` | 32,166,155 | — | Even smaller. Same quality ceiling as tiny. |
| **`base.en` (current)** | **147,964,211 / 142 MiB** | **~388 MB** | Fine speed. Weakest English quality among models OpenAI says actually benefit from `.en`. |
| `base.en-q5_1` | 59,721,011 | — | Smaller current default, not better. |
| `small.en` | 487,614,201 / 466 MiB | ~852 MB | Real quality step up. On A100 it is **slower than turbo** (4× vs 8×). |
| `small.en-q5_1` | 190,098,681 | — | ~181 MiB — closest “same download class as today, better WER.” |
| `medium.en` | 1,533,774,781 / 1.5 GiB | ~2.1 GB | Heavy next to Gemma. Turbo is faster and usually better. |
| `medium.en-q5_0` | 539,225,533 | — | Same disk class as turbo-q5, worse speed/quality trade. |
| `large-v3` | 3,095,033,483 / 2.9 GiB | ~3.9 GB | Best original Whisper. Too big/slow as a default beside Gemma. |
| `large-v3-q5_0` | 1,081,140,203 / 1.1 GiB | — | Still a 32-layer decoder. |
| **`large-v3-turbo`** | **1,624,555,275 / 1.5 GiB** | (not in the tiny→large mem table) | Full-precision turbo. Unneeded if q5 holds quality. |
| **`large-v3-turbo-q5_0`** | **574,041,195 / 547 MiB** | — | **Recommended default.** Official whisper.cpp quant. |
| `large-v3-turbo-q8_0` | 874,188,075 | — | Quality bump if q5 ever mishears; still < 1 GB. |
| `distil-large-v3` ggml | 1,519,521,155 | — | See below. Do not default. |

whisper.cpp memory table and model SHA list: [whisper.cpp README](https://github.com/ggml-org/whisper.cpp/blob/master/README.md), [models/README.md](https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md). Download script includes `large-v3-turbo`, `large-v3-turbo-q5_0`, `large-v3-turbo-q8_0`.

### Whisper `large-v3-turbo`

Primary sources:

- [openai/whisper discussion #2363](https://github.com/openai/whisper/discussions/2363) (2024-09-30 release)
- [Hugging Face `openai/whisper-large-v3-turbo`](https://huggingface.co/openai/whisper-large-v3-turbo)
- [openai/whisper README](https://github.com/openai/whisper/blob/main/README.md)
- [openai/whisper model card](https://github.com/openai/whisper/blob/main/model-card.md)

Facts from those, not blogs:

- Finetuned pruned `large-v3`: **decoder 32 layers → 4 layers** (same decoder depth as `tiny`). Encoder is still the large-v3 encoder.
- Trained two more epochs on the same multilingual **transcription** data as large-v3. **Not trained for translation.** OpenWhisper already sets `set_translate(false)`, so that limitation does not matter.
- OpenAI: performs similarly to `large-v2` across languages, with larger degradation on some (Thai, Cantonese). Better on FLEURS (cleaner audio) than Common Voice.
- OpenAI: with a scaled-dot-product-attention patch, **ASR speed of turbo is faster than what `tiny` used to be**, and they plot it as the speed/accuracy “best of both worlds.”
- Relative speed ~8× vs large, vs ~7× for `base` and ~4× for `small` on A100.

That last row is the whole argument against staying on `base.en` or moving only to `small.en`: turbo is in the tiny/base speed class with near-large quality.

**CPU caveat (important for short dictation):** turbo does **not** shrink the encoder. Whisper still encodes a 30-second padded window. On CPU, encoder cost can dominate a 3 s clip, so turbo can feel heavier than `small.en` / `base.en` despite the 4-layer decoder. OpenWhisper’s macOS release path is Metal, which is the case OpenAI and whisper.cpp optimized turbo for. Linux/Windows CPU-only installs are the exception, not the default we ship.

### Distil-Whisper

[distil-whisper/distil-large-v3](https://huggingface.co/distil-whisper/distil-large-v3) (Hugging Face, paper [arXiv:2311.00430](https://arxiv.org/abs/2311.00430)):

| Model | Params | Rel. latency | Short-form WER | Sequential long-form |
| --- | --- | --- | --- | --- |
| large-v3 | 1550 M | 1.0 | **8.4** | 10.0 |
| distil-large-v3 | 756 M | 6.3 | **9.7** | 10.8 |

OpenWhisper is short-form. Distil is **1.3 WER worse** on the metric that matches this product, and only modestly faster than turbo (~6.3× vs ~8× relative to large, different benches).

whisper.cpp’s own models README still says distilled models can be **sub-optimal in whisper.cpp** because the chunk-based strategy is not implemented. Distil-large-v3 was trained to work with sequential long-form (the algorithm whisper.cpp uses), but the upstream warning plus the short-form WER gap is enough to reject it as the default.

---

## Candidates that require a new engine

These beat Whisper on the “short utterance on a laptop” job, but they are not GGML Whisper weights. They are **not** the default until someone replaces `whisper-rs`.

### Moonshine (Useful Sensors, Oct 2024)

Sources: [arXiv:2410.15608](https://arxiv.org/abs/2410.15608), [Hugging Face `UsefulSensors/moonshine-tiny`](https://huggingface.co/UsefulSensors/moonshine-tiny).

- Encoder-decoder transformer with RoPE, trained **without zero-padding**. Encoder compute scales with audio length. Whisper always pays for 30 s.
- On 10 s of speech, **Moonshine Tiny is 5× less compute than Whisper `tiny.en`**, with no WER increase on standard sets.
- Tiny = 27.1 M params; Base = 61.5 M. Average Open ASR-style WER ≈ 12.66 (tiny) / 10.07 (base) vs Whisper tiny.en 12.81 / base.en 10.32.
- Built exactly for live transcription and voice commands on cheap devices. The paper’s motivation is a 500 ms floor on `tiny.en` caused by the padded encoder — the same floor OpenWhisper hits on every click.

Quality is **tiny/base-class**, not turbo-class. The win is latency architecture, not accuracy. Worth a later prototype (ONNX / Candle / a C++ port), not a default swap.

### NVIDIA Parakeet TDT 0.6B

Source: [nvidia/parakeet-tdt-0.6b-v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2) (v3 exists for 25 European languages).

- 600 M params, FastConformer + TDT, English, punctuation + capitalization + timestamps.
- Avg WER **6.05** on the HF Open ASR set (AMI 11.16, Earnings-22 11.15, GigaSpeech 9.74, LS clean 1.69, LS other 3.19, SPGI 2.17, TEDLIUM 3.38, VoxPopuli 5.95).
- Claimed RTFx **3380** on that leaderboard at batch 128. “At least 2 GB RAM to load.”
- Runtime is NeMo (Python) or community ports (e.g. sherpa-onnx). Not whisper.cpp.
- Built-in punctuation/caps would take some work off Gemma, but filler-word cleanup would still want the LLM.

Best accuracy/speed on paper. Wrong packaging for a Tauri/Rust binary that already vendors whisper.cpp and llama.cpp.

---

## Decision

**Default moving forward: `ggml-large-v3-turbo-q5_0.bin`**

```
https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin
```

SHA from whisper.cpp models README: `e050f7970618a659205450ad97eb95a18d69c9ee`. Disk **547 MiB**.

Why this one, given the constraints:

1. **Same engine.** whisper-rs loads it like any other GGML Whisper file. No new crate, no Python, no Core ML extra artifact.
2. **Speed class of tiny/base, quality class of large.** OpenAI’s published relative speed is ~8× vs large (~7× for base, ~4× for small). Decoder is 4 layers. This is the model OpenAI made the `whisper` CLI default.
3. **Size is small next to Gemma.** 547 MiB vs 141 MiB today vs ~5 GB for the refiner. Startup RAM is the Gemma problem, not this.
4. **`language: auto` starts meaning something.** `base.en` is English-only. Turbo is multilingual with large-v3’s encoder. Settings already expose a language field.
5. **q5_0 is the official published quant**, not a third-party conversion. If we ever see quality nits, `large-v3-turbo-q8_0` (834 MiB) is the fallback, not a jump to full 1.5 GiB F16.

What we are not picking, and why:

| Option | Verdict |
| --- | --- |
| Stay on `base.en` | Fast enough, but it is the size OpenAI says needs `.en` *because the multilingual twin is worse* — i.e. we are on the quality-constrained end. Gemma cannot un-mishear “send it to Alice” as “send it to Ellis.” |
| `small.en` / `small.en-q5_1` | Honest upgrade if turbo’s large encoder is slow on CPU-only boxes. On Metal it is the worse side of both speed and accuracy vs turbo-q5. Keep as a documented “low-RAM / CPU” option, not the default. |
| Distil-large-v3 | Worse short-form WER; whisper.cpp still warns. |
| Full `large-v3` | Pays 32 decoder layers we do not need. |
| Moonshine / Parakeet | Better long-term fits for dictation (variable-length encoder, or transducer + punctuation). Require a new runtime. Out of scope for a default-model change. |

### Suggested follow-ups (not blockers)

1. Point `ModelKind::Whisper` and `download-models.sh` at `ggml-large-v3-turbo-q5_0.bin`.
2. Keep Browse… so `small.en-q5_1` remains a one-click escape hatch for CPU-only machines.
3. Optional later: Core ML encoder (`ggml-large-v3-turbo-encoder.mlmodelc.zip` is already on the same HF repo) for ANE on Apple Silicon — extra packaging, not required to change the default.
4. Separate research if we ever leave whisper.cpp: Moonshine for the 30 s padding tax, Parakeet if we want punctuation without Gemma.

---

## Sources

- OpenAI Whisper README (model table, turbo default, `.en` note, no-translation caveat): https://github.com/openai/whisper/blob/main/README.md
- OpenAI Whisper model card (turbo 798 M, release dates): https://github.com/openai/whisper/blob/main/model-card.md
- OpenAI turbo release discussion: https://github.com/openai/whisper/discussions/2363
- Hugging Face `openai/whisper-large-v3-turbo` (32→4 decoder layers): https://huggingface.co/openai/whisper-large-v3-turbo
- whisper.cpp README (memory table, quantization, Core ML): https://github.com/ggml-org/whisper.cpp/blob/master/README.md
- whisper.cpp models README (disk sizes, SHAs, distil warning, turbo-q5_0): https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md
- Preconverted weights: https://huggingface.co/ggerganov/whisper.cpp
- Distil-Whisper large-v3 card (short-form WER 9.7 vs 8.4): https://huggingface.co/distil-whisper/distil-large-v3
- Distil-Whisper paper: https://arxiv.org/abs/2311.00430
- Moonshine paper: https://arxiv.org/abs/2410.15608
- Moonshine tiny card: https://huggingface.co/UsefulSensors/moonshine-tiny
- NVIDIA Parakeet TDT 0.6B v2 card (WER table, RTFx, 2 GB RAM): https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2
- This repo: `scripts/download-models.sh`, `src-tauri/src/models.rs`, `src-tauri/src/whisper_engine.rs`, `README.md`
