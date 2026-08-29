import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function simulateSetup() {
  const tick = async (
    key: "whisper" | "llm",
    totalMb: number,
    steps = 18
  ) => {
    const total = totalMb * 1024 * 1024;
    for (let i = 0; i <= steps; i++) {
      await emit("model-progress", {
        key,
        filename:
          key === "whisper"
            ? "ggml-small.en.bin"
            : "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        downloaded: Math.round((total * i) / steps),
        total,
        done: i === steps,
      });
      await sleep(90);
    }
  };

  await emit("setup-status", {
    stage: "whisper",
    message: "Downloading Whisper small.en…",
  });
  await tick("whisper", 465);

  await emit("setup-status", {
    stage: "llm",
    message: "Downloading Qwen3 4B Instruct 2507…",
  });
  await tick("llm", 2330, 24);

  await emit("setup-status", {
    stage: "load",
    message: "Loading models into memory…",
  });
  await sleep(700);

  await emit("setup-status", {
    stage: "mic",
    message: "Asking for microphone access…",
  });
  await sleep(500);

  await emit("setup-status", {
    stage: "accessibility",
    message: "Asking for Accessibility so text can paste…",
  });
  await sleep(500);

  await emit("setup-status", { stage: "ready", message: "Ready" });
}

const BROWSER_CONFIG = {
  hotkey: "Fn",
  whisper_model_path: "",
  llm_model_path: "",
  refine_prompt:
    "You are a dictation refinement assistant. Rewrite the following raw speech-to-text transcription as a clean, well-punctuated message. Remove filler words (um, uh, like, you know), false starts, and verbal tics. Preserve the speaker's intent, tone, and meaning exactly. Do not add new information. Output ONLY the rewritten message, with no preamble, no quotes, and no explanation.",
  use_llm_refinement: true,
  language: "auto",
  fast_paste: false,
  restore_clipboard: false,
  max_recording_seconds: 120,
  inference_threads: 0,
};

/** Browser-only stand-in so `npm run dev` can render the UI without Tauri. */
export function installBrowserMocks() {
  mockIPC((cmd) => {
    switch (cmd) {
      case "get_config":
        return { ...BROWSER_CONFIG };
      case "save_config":
      case "reload_models":
      case "dismiss_setup":
        return null;
      case "get_engine_status":
        return {
          whisper_loaded: false,
          whisper_error: null,
          llm_loaded: false,
          llm_error: null,
          hotkey: BROWSER_CONFIG.hotkey,
          hotkey_registered: false,
          hotkey_error: null,
        };
      case "list_models":
        return [
          {
            key: "whisper",
            filename: "ggml-small.en.bin",
            path: "",
            exists: false,
            size: 0,
          },
          {
            key: "llm",
            filename: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
            path: "",
            exists: false,
            size: 0,
          },
        ];
      case "start_setup":
        return simulateSetup();
      case "pick_file":
        return null;
      case "download_model":
        throw new Error("Model download needs the desktop app (`npm run tauri dev`).");
      case "test_dictate":
        throw new Error("Dictation needs the desktop app (`npm run tauri dev`).");
      default:
        return null;
    }
  }, { shouldMockEvents: true });
}
