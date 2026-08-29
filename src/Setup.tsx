import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { BRAND_EMOJI } from "./brand";

type Stage =
  | "starting"
  | "whisper"
  | "llm"
  | "load"
  | "mic"
  | "accessibility"
  | "ready"
  | "error";

interface SetupStatus {
  stage: Exclude<Stage, "starting">;
  message: string;
}

interface ModelProgress {
  key: "whisper" | "llm";
  downloaded: number;
  total: number;
  done: boolean;
}

interface Config {
  hotkey: string;
  use_llm_refinement: boolean;
}

function formatBytes(n: number): string {
  if (n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${units[i]}`;
}

const COPY: Record<
  Exclude<Stage, "ready" | "error">,
  { kicker: string; title: string; body: string }
> = {
  starting: {
    kicker: "First launch",
    title: "Setting up OpenWhisper",
    body: "Two models download once, then speech never leaves this computer.",
  },
  whisper: {
    kicker: "Speech model",
    title: "Teaching it to hear",
    body: "Whisper turns your voice into text. About 465 MB, one time.",
  },
  llm: {
    kicker: "Cleanup model",
    title: "Teaching it to tidy up",
    body: "Gemma strips filler words. The large download — then it stays offline.",
  },
  load: {
    kicker: "Warming up",
    title: "Loading into memory",
    body: "Almost ready. This can take a moment on the first launch.",
  },
  mic: {
    kicker: "Permission",
    title: "Allow the microphone",
    body: "macOS will ask so OpenWhisper can hear you. Nothing is uploaded.",
  },
  accessibility: {
    kicker: "Permission",
    title: "Allow paste",
    body: "Accessibility lets OpenWhisper drop text into whatever you were typing.",
  },
};

type StepId = "whisper" | "llm" | "load" | "permissions";

function stepsFor(useLlm: boolean): { id: StepId; label: string }[] {
  return [
    { id: "whisper", label: "Speech" },
    ...(useLlm ? [{ id: "llm" as const, label: "Cleanup" }] : []),
    { id: "load", label: "Load" },
    { id: "permissions", label: "Access" },
  ];
}

function mappedStep(stage: Stage): StepId {
  if (stage === "mic" || stage === "accessibility") return "permissions";
  if (stage === "llm" || stage === "load" || stage === "whisper") return stage;
  return "whisper";
}

function stepState(
  id: StepId,
  stage: Stage,
  useLlm: boolean
): "done" | "active" | "todo" {
  const order = stepsFor(useLlm).map((s) => s.id);
  const current = mappedStep(stage);
  const i = order.indexOf(id);
  const j = order.indexOf(current);
  if (stage === "ready") return "done";
  if (i < j) return "done";
  if (i === j) return "active";
  return "todo";
}

function Waveform() {
  return (
    <div className="setup-wave" aria-hidden="true">
      {Array.from({ length: 13 }, (_, i) => (
        <i key={i} style={{ animationDelay: `${i * 0.07}s` }} />
      ))}
    </div>
  );
}

function VoiceBloom({
  phase,
  hotkey,
}: {
  phase: "working" | "ready" | "error";
  hotkey: string;
}) {
  return (
    <div className={`setup-bloom ${phase}`} aria-hidden="true">
      <span className="setup-ring r1" />
      <span className="setup-ring r2" />
      <span className="setup-ring r3" />
      {phase === "ready" ? (
        <div className="setup-key">{hotkey}</div>
      ) : (
        <span className="setup-mark" aria-hidden="true">{BRAND_EMOJI}</span>
      )}
    </div>
  );
}

export default function Setup() {
  const [phase, setPhase] = useState<"working" | "ready" | "error">("working");
  const [stage, setStage] = useState<Stage>("starting");
  const [status, setStatus] = useState("Starting…");
  const [hotkey, setHotkey] = useState("Fn");
  const [useLlm, setUseLlm] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<ModelProgress | null>(null);
  const running = useRef(false);

  const run = async () => {
    if (running.current) return;
    running.current = true;
    setPhase("working");
    setStage("starting");
    setError(null);
    setProgress(null);
    setStatus("Starting…");
    try {
      await invoke("start_setup");
    } catch (e) {
      setPhase("error");
      setStage("error");
      setError(String(e));
    } finally {
      running.current = false;
    }
  };

  useEffect(() => {
    const un: UnlistenFn[] = [];
    let stop = false;

    (async () => {
      try {
        const cfg = await invoke<Config>("get_config");
        if (!stop) {
          if (cfg.hotkey) setHotkey(cfg.hotkey);
          setUseLlm(cfg.use_llm_refinement !== false);
        }
      } catch {
        /* keep defaults */
      }

      un.push(
        await listen<SetupStatus>("setup-status", (e) => {
          setStatus(e.payload.message);
          setStage(e.payload.stage);
          if (e.payload.stage === "ready") setPhase("ready");
          if (e.payload.stage === "error") {
            setPhase("error");
            setError(e.payload.message);
          }
          if (e.payload.stage !== "whisper" && e.payload.stage !== "llm") {
            setProgress(null);
          }
        })
      );
      un.push(
        await listen<ModelProgress>("model-progress", (e) => {
          setProgress(e.payload);
        })
      );

      if (!stop) await run();
    })();

    return () => {
      stop = true;
      un.forEach((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const pct =
    progress && progress.total > 0
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : null;

  const dismiss = async () => {
    try {
      await invoke("dismiss_setup");
    } catch {
      /* window may already be gone */
    }
  };

  const copy =
    phase === "ready"
      ? {
          kicker: "You're ready",
          title: `Hold ${hotkey} and speak`,
          body: "Release to paste into whatever you were typing. OpenWhisper lives in the menu bar.",
        }
      : phase === "error"
        ? {
            kicker: "Couldn't finish",
            title: "Setup paused",
            body: error || "Something went wrong.",
          }
        : COPY[stage === "error" || stage === "ready" ? "starting" : stage];

  return (
    <div className={`setup setup-${phase}`}>
      <div className="setup-card">
        <VoiceBloom phase={phase} hotkey={hotkey} />
        {phase === "working" && <Waveform />}
        {phase === "ready" && (
          <p className="setup-hold" aria-hidden="true">
            hold to talk
          </p>
        )}

        <p className="setup-kicker">{copy.kicker}</p>
        <h1>{copy.title}</h1>
        <p className="setup-copy">{copy.body}</p>

        {phase === "working" && (
          <>
            <ol className={`setup-steps n-${stepsFor(useLlm).length}`}>
              {stepsFor(useLlm).map((s) => {
                const st = stepState(s.id, stage, useLlm);
                return (
                  <li key={s.id} className={`setup-step ${st}`}>
                    <span className="setup-dot" />
                    <span className="setup-step-label">{s.label}</span>
                  </li>
                );
              })}
            </ol>

            <div
              className={`setup-bar ${pct === null ? "indeterminate" : ""}`}
            >
              <div
                className="setup-bar-fill"
                style={pct === null ? undefined : { width: `${pct}%` }}
              />
            </div>

            <p className="setup-status">{status}</p>
            {progress && progress.total > 0 && (
              <p className="setup-bytes">
                {formatBytes(progress.downloaded)} of{" "}
                {formatBytes(progress.total)}
                {pct !== null ? ` · ${pct}%` : ""}
              </p>
            )}
          </>
        )}

        {phase === "ready" && (
          <button className="btn setup-done" onClick={dismiss}>
            Start dictating
          </button>
        )}

        {phase === "error" && (
          <button
            className="btn setup-done"
            onClick={() => {
              running.current = false;
              run();
            }}
          >
            Try again
          </button>
        )}

        <p className="setup-local">
          <svg
            width="12"
            height="12"
            viewBox="0 0 12 12"
            aria-hidden="true"
          >
            <rect
              x="2.5"
              y="5.5"
              width="7"
              height="5"
              rx="1.2"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.2"
            />
            <path
              d="M4 5.5V4a2 2 0 0 1 4 0v1.5"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.2"
              strokeLinecap="round"
            />
          </svg>
          Stays on this computer
        </p>
      </div>
    </div>
  );
}
