# OpenWhisper

A fully-local, cross-platform clone of WhisperFlow. Click-to-talk dictation
from the menu bar with a local Whisper model for transcription and a local
Gemma 4 E2B for LLM-powered cleanup. Nothing leaves your machine.

## How it works

```
[Click tray icon] -> mic capture (cpal, 16kHz mono)
[Click tray icon] -> Whisper.cpp transcription (whisper-rs)
                  -> Gemma 4 E2B refinement (llama.cpp via llama-cpp-2)
                  -> Paste into focused text field, or copy to clipboard
```

Click the menu-bar icon to start listening (it shows `● REC`), click again to
stop. The raw transcription is rewritten by Gemma into a clean message
("um, like, can you, uh, send him a message saying I'll be late" → "Can you
send him a message saying I'll be late?"). If a text field is focused, it's
pasted there; otherwise it's left on the clipboard for you to paste manually.

A global hold-to-talk hotkey (default `Fn`) is also
registered as an alternative trigger.

## Stack

- **Tauri 2** (Rust + system webview) for a small cross-platform binary
- **whisper-rs** (whisper.cpp bindings) for STT
- **llama-cpp-2** (llama.cpp bindings) for the local LLM
- **cpal** for cross-platform audio capture
- **enigo** for cross-platform keyboard injection
- **tauri-plugin-global-shortcut** for the push-to-talk hotkey
- **React + Vite** for the minimal overlay/settings UI

## Build & run

### Prerequisites

- Rust (stable) and `cargo`
- Node 20+ and npm/pnpm
- A C/C++ toolchain (Xcode CLT on macOS, MSVC on Windows, build-essential on Linux)
- CMake (required by whisper.cpp / llama.cpp)

### One-time setup

```bash
npm install
./scripts/download-models.sh   # downloads Whisper base.en + Gemma-4-E2B-it Q4_K_M
```

### Dev

```bash
npm run tauri dev
```

For Apple Silicon GPU acceleration (Metal) on macOS:

```bash
npm run tauri dev -- --features metal
```

For NVIDIA:

```bash
npm run tauri dev -- --features cuda
```

### Production build

```bash
npm run tauri build           # CPU build
npm run tauri build -- --features metal   # macOS Metal
```

This produces both an installable `.app` and a `.dmg`:

```
src-tauri/target/release/bundle/macos/OpenWhisper.app
src-tauri/target/release/bundle/dmg/OpenWhisper_<version>_aarch64.dmg
```

## Releasing (maintainers)

Signed + notarized releases are published to GitHub with one command:

```bash
npm run release                 # build (metal), sign, notarize, staple, publish
npm run release -- --no-upload  # build + verify locally, skip GitHub upload
npm run release -- --version 0.2.0 --force-tag
```

`scripts/release.mjs` runs `tauri build`, signs the `.app` with the local
**Developer ID Application** identity, notarizes and staples both the app and a
freshly built DMG via Apple's `notarytool`, verifies everything with `spctl`,
then tags the commit and uploads the DMG + zipped app to a GitHub release.

Requirements on the build machine:

- A *Developer ID Application* identity in the login Keychain (auto-detected).
- A `notarytool` keychain profile (default `beside`; override with
  `--notary-profile <name>` or `APPLE_KEYCHAIN_PROFILE`). You can also use Apple
  API key env vars (`APPLE_API_KEY` / `APPLE_API_KEY_ID` / `APPLE_API_ISSUER`)
  or Apple ID env vars (`APPLE_ID` / `APPLE_APP_SPECIFIC_PASSWORD` /
  `APPLE_TEAM_ID`).
- `gh` authenticated with `repo` scope.

To cut a new release, bump `version` in both `package.json` and
`src-tauri/tauri.conf.json` (they must match), commit, then run
`npm run release`. Run `npm run release -- --help` for all flags.

## Installing on macOS

1. Download the latest `.dmg` from the
   [Releases](https://github.com/sagivo/openwhisper/releases) page.
2. Open the `.dmg` and drag **OpenWhisper.app** into `/Applications`.
3. Release builds are **signed with a Developer ID and notarized by Apple**, so
   they launch normally with a double-click — no right-click override needed.
5. OpenWhisper is a menu-bar-only app (`LSUIElement`) — there's no Dock icon.
   Look for the 🗣️ icon in the menu bar.
6. macOS will prompt for **Microphone** access on the first dictation, and
   you'll need to grant **Accessibility** access in
   *System Settings → Privacy & Security → Accessibility* so the app can
   simulate keystrokes into the focused text field.

## First-run setup

The Settings window has a **Models** section that downloads the default
Whisper (≈148 MB) and Gemma 4 E2B (≈3.1 GB) models into
`~/Library/Application Support/openwhisper/models/` and wires them into the
config automatically. Click each **Download** button once and you're done.

If you want to point at custom models instead, use the **Whisper Model** and
**Gemma Model** path fields and click **Save & Reload**.

You'll need to grant **microphone** and **accessibility** permissions on
macOS the first time you trigger the hotkey (the OS will prompt). On Linux,
keyboard injection uses XTest/uinput depending on display server.

## Settings

| Field | Default | Notes |
| --- | --- | --- |
| Hotkey | `Fn` | Push-to-talk. Hold to record. |
| Whisper model | _empty_ | Any GGML/GGUF whisper.cpp model. |
| Language | `auto` | Whisper language hint, e.g. `en`, `es`, or `auto`. |
| Refine with Gemma | on | Disable to type the raw transcript verbatim. |
| Gemma model | _empty_ | Any GGUF Gemma model. Gemma 4 E2B Q4_K_M is recommended. |
| Refine prompt | (sensible default) | Editable system prompt for the LLM. |
| Fast paste | on | Paste the raw transcript immediately, then replace it with the refined version when Gemma finishes (skipped if refinement takes >2 s, to avoid clobbering anything you've typed since). |

Config lives at `~/Library/Application Support/openwhisper/config.json`
(macOS), `~/.config/openwhisper/config.json` (Linux), or
`%APPDATA%\openwhisper\config.json` (Windows).

## Why this is efficient

- **Single binary, no Python.** Whisper and Gemma run via native
  C++ bindings.
- **Models are loaded once** on startup and reused for every dictation. The
  llama.cpp context is also kept warm across calls and the system-prompt
  prefix is cached in the KV cache, so refinement only re-decodes the small
  variable suffix (transcript) on each invocation.
- **No network calls** during dictation. Audio, transcript, and refined text
  never leave the process. (Optional update check pings GitHub Releases on
  demand.)
- **Greedy sampling, low max-tokens** for the LLM keeps refinement under
  ~500 ms on Apple Silicon with the 2B Q4_K_M model.
- **Fast-paste mode** types the raw transcript the instant Whisper finishes,
  then replaces it with the refined version once Gemma is done. Perceived
  latency is effectively zero.
- **Anti-aliased downsampling** to 16 kHz via a windowed-sinc low-pass
  before linear interpolation, so 48 kHz mics don't fold high-frequency
  noise into the speech band.
- **Frame-level VAD** trims leading/trailing silence and rejects all-silent
  clips before they reach Whisper, eliminating the "Thanks for watching!"
  hallucinations the previous global RMS gate sometimes missed.
- **Clipboard is restored** after every paste so OpenWhisper doesn't clobber
  whatever you had copied.
- **GPU acceleration** is opt-in via Cargo features (`metal`, `cuda`).

## Project layout

```
src/                React UI (Settings window)
src-tauri/
  src/
    lib.rs              orchestration, hotkey, Tauri commands
    audio.rs            cpal capture, downmix, resample to 16kHz
    whisper_engine.rs   whisper-rs wrapper
    llm_engine.rs       llama-cpp-2 wrapper, Gemma chat template
    injector.rs         enigo text typing
    config.rs           on-disk JSON config
  tauri.conf.json
scripts/download-models.sh
```

## Notes

- `llama-cpp-2` is pinned to **0.1.133**. Newer versions (0.1.134+) bundle a
  llama.cpp build that asserts `LLAMA_TENSOR_NAME_FGDN_AR` during graph
  reservation for Gemma 2's hybrid SWA layers. 0.1.133 predates that
  regression and runs Gemma 2 cleanly on Metal and CPU.
- Whisper has a tendency to hallucinate text from background noise. We apply
  a simple RMS energy gate (≥ 0.005) before transcription; truly silent
  recordings return an empty string. For windy mics, sentences like "Thanks
  for watching!" can still slip through — just don't talk if you don't mean it.

## License

MIT.
