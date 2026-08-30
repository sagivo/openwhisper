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
  filename?: string;
  downloaded: number;
  total: number;
  done: boolean;
}

interface Config {
  hotkey: string;
  use_llm_refinement: boolean;
}

interface HotkeyAccess {
  accessibility: boolean;
  input_monitoring: boolean;
}

interface StatusEvent {
  kind: string;
  message?: string;
}

type Trial =
  | { step: "idle" }
  | { step: "checking" }
  | { step: "listening" }
  | { step: "working" }
  | { step: "success"; text: string }
  | { step: "miss" }
  | { step: "needs-access"; accessibility: boolean; input_monitoring: boolean }
  | { step: "error"; message: string };

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
    body: "Downloading Whisper small.en. About 465 MB, one time.",
  },
  llm: {
    kicker: "Local AI model",
    title: "Teaching it to tidy up",
    body: "Downloading Qwen3 4B Instruct 2507. About 2.3 GB, one time.",
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
    ...(useLlm ? [{ id: "llm" as const, label: "Local AI" }] : []),
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

function accessCopy(accessibility: boolean, inputMonitoring: boolean): string {
  if (!accessibility && !inputMonitoring) {
    return "Allow Accessibility and Input Monitoring in System Settings, then click the key again.";
  }
  if (!accessibility) {
    return "Allow Accessibility in System Settings so OpenWhisper can paste, then click the key again.";
  }
  return "Allow Input Monitoring in System Settings so OpenWhisper can hear the key, then click it again.";
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

function ConfettiBurst({ play }: { play: number }) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (!play) return;
    const canvas = ref.current;
    if (!canvas) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const colors = ["#7c6cff", "#a78bfa", "#5eead4", "#f5f5f7", "#f9a8d4"];
    const pieces = Array.from({ length: 80 }, () => ({
      x: rect.width * 0.5 + (Math.random() - 0.5) * 36,
      y: 88,
      vx: (Math.random() - 0.5) * 10,
      vy: -5 - Math.random() * 8,
      w: 4 + Math.random() * 5,
      h: 6 + Math.random() * 7,
      rot: Math.random() * Math.PI,
      vr: (Math.random() - 0.5) * 0.28,
      color: colors[Math.floor(Math.random() * colors.length)]!,
    }));
    const g = 0.17;
    let raf = 0;
    const t0 = performance.now();
    const tick = (now: number) => {
      const t = now - t0;
      ctx.clearRect(0, 0, rect.width, rect.height);
      for (const p of pieces) {
        p.vy += g;
        p.x += p.vx;
        p.y += p.vy;
        p.rot += p.vr;
        ctx.save();
        ctx.translate(p.x, p.y);
        ctx.rotate(p.rot);
        ctx.globalAlpha = Math.max(0, 1 - t / 1700);
        ctx.fillStyle = p.color;
        ctx.fillRect(-p.w / 2, -p.h / 2, p.w, p.h);
        ctx.restore();
      }
      if (t < 1700) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [play]);

  return <canvas ref={ref} className="setup-confetti" aria-hidden="true" />;
}

function VoiceBloom({
  phase,
  hotkey,
  onPress,
  onRelease,
  listening,
}: {
  phase: "working" | "ready" | "error";
  hotkey: string;
  onPress?: () => void;
  onRelease?: () => void;
  listening?: boolean;
}) {
  const interactive = phase === "ready" && Boolean(onPress);
  return (
    <div
      className={`setup-bloom ${phase}${listening ? " listening" : ""}`}
      aria-hidden={interactive ? undefined : true}
      onPointerDown={(e) => {
        if (!interactive) return;
        e.preventDefault();
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        onPress?.();
      }}
      onPointerUp={() => {
        if (!interactive) return;
        onRelease?.();
      }}
      onPointerCancel={() => {
        if (!interactive) return;
        onRelease?.();
      }}
    >
      <span className="setup-ring r1" />
      <span className="setup-ring r2" />
      <span className="setup-ring r3" />
      {phase === "ready" ? (
        <div className="setup-key" aria-label={`Hold ${hotkey}`}>
          {hotkey}
        </div>
      ) : (
        <span className="setup-mark" aria-hidden="true">
          {BRAND_EMOJI}
        </span>
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
  const [trial, setTrial] = useState<Trial>({ step: "idle" });
  const [burst, setBurst] = useState(0);
  const running = useRef(false);
  const holding = useRef(false);
  const phaseRef = useRef(phase);
  phaseRef.current = phase;

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
      un.push(
        await listen<StatusEvent>("status", (e) => {
          if (phaseRef.current !== "ready") return;
          if (e.payload.kind === "recording") setTrial({ step: "listening" });
          if (e.payload.kind === "transcribing" || e.payload.kind === "refining") {
            setTrial({ step: "working" });
          }
          if (e.payload.kind === "error") {
            setTrial({
              step: "error",
              message: e.payload.message || "Something went wrong.",
            });
          }
        })
      );
      un.push(
        await listen<string>("dictation-result", (e) => {
          if (phaseRef.current !== "ready") return;
          const text = (e.payload || "").trim();
          if (text) {
            setTrial({ step: "success", text });
            setBurst((n) => n + 1);
          } else {
            setTrial({ step: "miss" });
          }
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

  const onPress = async () => {
    if (holding.current) return;
    holding.current = true;
    setTrial({ step: "checking" });
    try {
      const access = await invoke<HotkeyAccess>("hotkey_press");
      if (!holding.current) {
        await invoke("hotkey_release");
        return;
      }
      if (!access.accessibility || !access.input_monitoring) {
        holding.current = false;
        setTrial({
          step: "needs-access",
          accessibility: access.accessibility,
          input_monitoring: access.input_monitoring,
        });
      }
    } catch (e) {
      holding.current = false;
      setTrial({ step: "error", message: String(e) });
    }
  };

  const onRelease = async () => {
    holding.current = false;
    try {
      await invoke("hotkey_release");
    } catch {
      /* not recording */
    }
  };

  const trialBusy =
    trial.step === "checking" ||
    trial.step === "listening" ||
    trial.step === "working";

  const copy =
    phase === "ready"
      ? trial.step === "checking"
        ? {
            kicker: "Checking access",
            title: `Making sure ${hotkey} works`,
            body: "macOS will ask if OpenWhisper still needs permission to hear the key.",
          }
        : trial.step === "listening"
          ? {
              kicker: "Listening",
              title: "Speak now",
              body: "Say something. We'll transcribe it here so you know it's working.",
            }
          : trial.step === "working"
            ? {
                kicker: "Got it",
                title: "Turning that into text",
                body: "Speech stays on this computer.",
              }
            : trial.step === "success"
              ? {
                  kicker: "It works",
                  title: trial.text,
                  body: `That's what OpenWhisper heard. Hold ${hotkey} anywhere to dictate.`,
                }
              : trial.step === "miss"
                ? {
                    kicker: "Didn't catch that",
                    title: "Try again",
                    body: `Hold ${hotkey} and speak while it listens.`,
                  }
                : trial.step === "needs-access"
                  ? {
                      kicker: "Permission needed",
                      title: `Allow access to ${hotkey}`,
                      body: accessCopy(
                        trial.accessibility,
                        trial.input_monitoring
                      ),
                    }
                  : trial.step === "error"
                    ? {
                        kicker: "Couldn't hear you",
                        title: "Try again",
                        body: trial.message,
                      }
                    : {
                        kicker: "You're ready",
                        title: `Hold ${hotkey} and speak`,
                        body: "Hold the key on your keyboard — or press the one above — then speak. We'll show what we heard.",
                      }
      : phase === "error"
        ? {
            kicker: "Couldn't finish",
            title: "Setup paused",
            body: error || "Something went wrong.",
          }
        : COPY[stage === "error" || stage === "ready" ? "starting" : stage];

  return (
    <div
      className={`setup setup-${phase}${trial.step === "success" ? " setup-celebrating" : ""}`}
    >
      <div className="setup-card">
        {burst > 0 && <ConfettiBurst play={burst} />}
        <VoiceBloom
          phase={phase}
          hotkey={hotkey}
          onPress={phase === "ready" ? onPress : undefined}
          onRelease={phase === "ready" ? onRelease : undefined}
          listening={trial.step === "listening"}
        />
        {(phase === "working" || trial.step === "listening") && <Waveform />}
        {phase === "ready" && !trialBusy && (
          <p className="setup-hold" aria-hidden="true">
            hold to talk
          </p>
        )}

        {(phase === "working" || phase === "error") && (
          <p className="setup-once">First install only</p>
        )}
        <p className="setup-kicker">{copy.kicker}</p>
        <h1 className={trial.step === "success" ? "setup-result" : undefined}>
          {copy.title}
        </h1>
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
                {progress.filename ? `${progress.filename} · ` : ""}
                {formatBytes(progress.downloaded)} of{" "}
                {formatBytes(progress.total)}
                {pct !== null ? ` · ${pct}%` : ""}
              </p>
            )}
          </>
        )}

        {phase === "ready" && (
          <button className="btn setup-done" onClick={dismiss} disabled={trialBusy}>
            Start dictating
          </button>
        )}

        {phase === "ready" && trial.step === "needs-access" && (
          <div className="setup-perms">
            <button
              className="btn setup-done"
              onClick={() =>
                invoke("open_permission_pane", {
                  pane: trial.accessibility ? "input-monitoring" : "accessibility",
                })
              }
            >
              {trial.accessibility
                ? "Open Input Monitoring settings"
                : "Open Accessibility settings"}
            </button>
            <p className="setup-restart-hint">
              Toggled it on but still nothing? Quit OpenWhisper from the menu
              bar (🤫 → Quit) and reopen it. If OpenWhisper is already listed
              but the toggle won't stick, remove it from the list, reopen
              OpenWhisper, and allow it when macOS asks.
            </p>
          </div>
        )}

        {phase === "ready" && trial.step === "error" && (
          <p className="setup-restart-hint">
            If this mentions the microphone: allow OpenWhisper under System
            Settings → Privacy &amp; Security → Microphone, then quit and
            reopen the app.
          </p>
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
