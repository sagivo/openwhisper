import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";

interface Config {
  hotkey: string;
  whisper_model_path: string;
  llm_model_path: string;
  refine_prompt: string;
  use_llm_refinement: boolean;
  language: string;
  fast_paste: boolean;
  restore_clipboard: boolean;
  max_recording_seconds: number;
  inference_threads: number;
}

interface VersionInfo {
  current: string;
  latest: string | null;
  update_available: boolean;
  release_url: string | null;
}

interface EngineStatus {
  whisper_loaded: boolean;
  whisper_error: string | null;
  llm_loaded: boolean;
  llm_error: string | null;
  hotkey: string;
  hotkey_registered: boolean;
  hotkey_error: string | null;
}

type CheckState = "idle" | "checking" | "ok" | "fail" | "off";

interface Check {
  key: "mic" | "hotkey" | "whisper" | "llm";
  label: string;
  state: CheckState;
  detail: string | null;
}

function formatLoadError(e: unknown): string {
  const s = String(e);
  if (s.includes("__TAURI_INTERNALS__") || s.includes("is not a function")) {
    return "This window is the desktop UI. Run `npm run tauri dev` so settings can talk to the backend.";
  }
  return s;
}

export default function Settings() {
  const [cfg, setCfg] = useState<Config | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [engine, setEngine] = useState<EngineStatus | null>(null);

  const [version, setVersion] = useState<VersionInfo | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);

  const [micState, setMicState] = useState<CheckState>("idle");
  const [micDetail, setMicDetail] = useState<string | null>(null);
  const [runningCheck, setRunningCheck] = useState(false);

  const [hotkeyDraft, setHotkeyDraft] = useState<string>("");
  const [savingHotkey, setSavingHotkey] = useState(false);
  const [hotkeyMsg, setHotkeyMsg] = useState<string | null>(null);

  const [promptDraft, setPromptDraft] = useState<string>("");
  const [savingPrompt, setSavingPrompt] = useState(false);
  const [promptMsg, setPromptMsg] = useState<string | null>(null);

  useEffect(() => {
    invoke<Config>("get_config")
      .then((c) => {
        setCfg(c);
        setHotkeyDraft(c.hotkey);
        setPromptDraft(c.refine_prompt);
        setLoadError(null);
      })
      .catch((e) => setLoadError(formatLoadError(e)));
    invoke<EngineStatus>("get_engine_status").then(setEngine).catch(() => {});
    invoke<VersionInfo>("app_version").then(setVersion).catch(() => {});
    const unEngine = listen<EngineStatus>("engine-status", (e) =>
      setEngine(e.payload)
    );
    return () => {
      unEngine.then((fn) => fn());
    };
  }, []);

  const checks: Check[] = [
    {
      key: "mic",
      label: "Microphone",
      state: micState,
      detail: micDetail,
    },
    {
      key: "hotkey",
      label: "Key binding",
      state: engine
        ? engine.hotkey_registered
          ? "ok"
          : "fail"
        : "idle",
      detail: engine?.hotkey_error
        ? `"${engine.hotkey}" failed to register: ${engine.hotkey_error}`
        : engine?.hotkey_registered
        ? `Listening on ${engine.hotkey}`
        : null,
    },
    {
      key: "whisper",
      label: "Whisper model",
      state: engine
        ? engine.whisper_loaded
          ? "ok"
          : "fail"
        : "idle",
      detail: engine?.whisper_error ?? (engine?.whisper_loaded ? "Loaded" : null),
    },
    {
      key: "llm",
      label: "Local AI (refinement)",
      state: !cfg?.use_llm_refinement
        ? "off"
        : engine
        ? engine.llm_loaded
          ? "ok"
          : "fail"
        : "idle",
      detail: cfg?.use_llm_refinement
        ? engine?.llm_error ?? (engine?.llm_loaded ? "Loaded" : null)
        : "Refinement is disabled — transcripts are pasted raw.",
    },
  ];

  const runCheck = async () => {
    setRunningCheck(true);
    setMicState("checking");
    setMicDetail(null);
    try {
      await invoke("check_mic");
      setMicState("ok");
      setMicDetail("Default input device is working");
    } catch (e) {
      setMicState("fail");
      setMicDetail(String(e));
    }
    try {
      setEngine(await invoke<EngineStatus>("get_engine_status"));
    } catch {
      /* engine-status listener keeps this fresh anyway */
    }
    setRunningCheck(false);
  };

  const checkUpdates = async () => {
    setCheckingUpdates(true);
    setUpdateMsg("Checking for updates…");
    try {
      const next = await invoke<VersionInfo>("check_for_updates");
      setVersion(next);
      setUpdateMsg(
        next.update_available && next.latest
          ? `Update available: v${next.latest}`
          : "You're on the latest version."
      );
    } catch (e) {
      setUpdateMsg("Update check failed: " + String(e));
    } finally {
      setCheckingUpdates(false);
    }
  };

  const saveHotkey = async () => {
    if (!cfg) return;
    setSavingHotkey(true);
    setHotkeyMsg(null);
    try {
      await invoke("save_config", { config: { ...cfg, hotkey: hotkeyDraft } });
      setCfg({ ...cfg, hotkey: hotkeyDraft });
      setHotkeyMsg(`Saved — holding ${hotkeyDraft} now dictates.`);
    } catch (e) {
      setHotkeyMsg(String(e));
    } finally {
      setSavingHotkey(false);
    }
  };

  const savePrompt = async () => {
    if (!cfg) return;
    setSavingPrompt(true);
    setPromptMsg(null);
    try {
      await invoke("save_config", {
        config: { ...cfg, refine_prompt: promptDraft },
      });
      setCfg({ ...cfg, refine_prompt: promptDraft });
      setPromptMsg("Saved.");
    } catch (e) {
      setPromptMsg(String(e));
    } finally {
      setSavingPrompt(false);
    }
  };

  const resetPrompt = async () => {
    try {
      const def = await invoke<string>("default_prompt");
      setPromptDraft(def);
    } catch (e) {
      setPromptMsg(String(e));
    }
  };

  if (loadError && !cfg) {
    return (
      <div className="settings">
        <h1>OpenWhisper</h1>
        <p className="sub">{loadError}</p>
      </div>
    );
  }
  if (!cfg) return <div className="settings">Loading…</div>;

  const dotClass = (s: CheckState) =>
    s === "ok" || s === "off" ? "ok" : s === "fail" ? "fail" : "pending";
  const stateLabel = (s: CheckState) =>
    s === "ok"
      ? "OK"
      : s === "off"
      ? "Off"
      : s === "fail"
      ? "Problem"
      : s === "checking"
      ? "Checking…"
      : "Not checked";

  return (
    <div className="settings">
      <h1>OpenWhisper</h1>
      <p className="sub">
        Hold <span className="kbd">{cfg.hotkey}</span> to dictate. Release to
        transcribe and type into the focused app.
      </p>

      {/* ---- Version ---- */}
      <section className="section">
        <div className="section-head">
          <h2>Version</h2>
          <button
            className="btn secondary small"
            disabled={checkingUpdates}
            onClick={checkUpdates}
          >
            {checkingUpdates ? "Checking…" : "Check for updates"}
          </button>
        </div>
        <div className="section-body">
          <div className="version-line">
            <span className="about-value">v{version?.current ?? "…"}</span>
            {version?.update_available && version.release_url && (
              <button
                className="btn small"
                onClick={() =>
                  open(version.release_url!).catch(() =>
                    window.open(version.release_url!, "_blank")
                  )
                }
              >
                Download v{version.latest}
              </button>
            )}
          </div>
          {updateMsg && <p className="hint">{updateMsg}</p>}
        </div>
      </section>

      {/* ---- Health check ---- */}
      <section className="section">
        <div className="section-head">
          <h2>Everything connected?</h2>
          <button
            className="btn secondary small"
            disabled={runningCheck}
            onClick={runCheck}
          >
            {runningCheck ? "Running…" : "Run check"}
          </button>
        </div>
        <div className="section-body">
          <div className="checks">
            {checks.map((c) => (
              <div key={c.key} className="check-row">
                <span className={`dot dot-${dotClass(c.state)}`} />
                <div className="check-text">
                  <div className="check-label">{c.label}</div>
                  {c.detail && <div className="check-detail">{c.detail}</div>}
                </div>
                <span className={`badge${c.state === "ok" || c.state === "off" ? " ok" : c.state === "fail" ? " bad" : ""}`}>
                  {stateLabel(c.state)}
                </span>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* ---- Key binding ---- */}
      <section className="section">
        <div className="section-head">
          <h2>Key binding</h2>
        </div>
        <div className="section-body">
          <div className="inline-field">
            <input
              type="text"
              value={hotkeyDraft}
              onChange={(e) => setHotkeyDraft(e.target.value)}
              placeholder="e.g. Fn or CmdOrCtrl+Shift+Space"
            />
            <button
              className="btn small"
              disabled={savingHotkey || hotkeyDraft === cfg.hotkey}
              onClick={saveHotkey}
            >
              {savingHotkey ? "Saving…" : "Save"}
            </button>
          </div>
          <p className="hint">
            Hold-to-talk is global — it works in any app. Combos like
            Cmd+Shift+Space may conflict with other apps.
          </p>
          {hotkeyMsg && <p className="hint">{hotkeyMsg}</p>}
        </div>
      </section>

      {/* ---- System prompt ---- */}
      <section className="section">
        <div className="section-head">
          <h2>System prompt</h2>
          <button className="btn secondary small" onClick={resetPrompt}>
            Reset to default
          </button>
        </div>
        <div className="section-body">
          <textarea
            value={promptDraft}
            onChange={(e) => setPromptDraft(e.target.value)}
            rows={6}
          />
          <div className="prompt-actions">
            <button
              className="btn small"
              disabled={savingPrompt || promptDraft === cfg.refine_prompt}
              onClick={savePrompt}
            >
              {savingPrompt ? "Saving…" : "Save"}
            </button>
            {promptMsg && <span className="hint">{promptMsg}</span>}
          </div>
          <p className="hint">
            Applied to the local AI that cleans up your transcripts.
          </p>
        </div>
      </section>
    </div>
  );
}
