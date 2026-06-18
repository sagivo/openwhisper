import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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

interface ModelStatus {
  key: "whisper" | "llm";
  filename: string;
  path: string;
  exists: boolean;
  size: number;
}

interface ModelProgress {
  key: "whisper" | "llm";
  downloaded: number;
  total: number;
  done: boolean;
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

const MODEL_LABELS: Record<ModelStatus["key"], { name: string; subtitle: string }> = {
  whisper: {
    name: "Whisper (base.en)",
    subtitle: "~148 MB · speech-to-text",
  },
  llm: {
    name: "Gemma 4 E4B Instruct (Q4_K_M)",
    subtitle: "~5.0 GB · transcript cleanup",
  },
};

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

export default function Settings() {
  const [cfg, setCfg] = useState<Config | null>(null);
  const [log, setLog] = useState<string>("");
  const [saving, setSaving] = useState(false);
  const [modelStatus, setModelStatus] = useState<ModelStatus[]>([]);
  const [progress, setProgress] = useState<Record<string, ModelProgress>>({});
  const [downloading, setDownloading] = useState<Record<string, boolean>>({});
  const [engine, setEngine] = useState<EngineStatus | null>(null);

  const refreshModels = async () => {
    try {
      const list = await invoke<ModelStatus[]>("list_models");
      setModelStatus(list);
    } catch (e) {
      setLog((p) => p + "\n" + String(e));
    }
  };

  useEffect(() => {
    invoke<Config>("get_config").then(setCfg).catch((e) => setLog(String(e)));
    invoke<EngineStatus>("get_engine_status").then(setEngine).catch(() => {});
    refreshModels();
    const unLog = listen<string>("log", (e) =>
      setLog((prev) => (prev + "\n" + e.payload).split("\n").slice(-20).join("\n"))
    );
    const unProg = listen<ModelProgress>("model-progress", (e) => {
      setProgress((prev) => ({ ...prev, [e.payload.key]: e.payload }));
      if (e.payload.done) {
        setDownloading((prev) => ({ ...prev, [e.payload.key]: false }));
        refreshModels();
      }
    });
    const unEngine = listen<EngineStatus>("engine-status", (e) =>
      setEngine(e.payload)
    );
    return () => {
      unLog.then((fn) => fn());
      unProg.then((fn) => fn());
      unEngine.then((fn) => fn());
    };
  }, []);

  const downloadModel = async (key: ModelStatus["key"]) => {
    setDownloading((prev) => ({ ...prev, [key]: true }));
    setProgress((prev) => ({
      ...prev,
      [key]: { key, downloaded: 0, total: 0, done: false },
    }));
    try {
      await invoke<string>("download_model", { kind: key });
      setLog((p) => p + `\nDownloaded ${key} model.`);
      const fresh = await invoke<Config>("get_config");
      setCfg(fresh);
      await invoke("reload_models");
    } catch (e) {
      setLog((p) => p + "\nDownload failed: " + String(e));
    } finally {
      setDownloading((prev) => ({ ...prev, [key]: false }));
      refreshModels();
    }
  };

  if (!cfg) return <div className="settings">Loading…</div>;

  const update = <K extends keyof Config>(k: K, v: Config[K]) =>
    setCfg({ ...cfg, [k]: v });

  const updateNumber = (
    key: "max_recording_seconds" | "inference_threads",
    value: string
  ) => {
    const parsed = Number.parseInt(value, 10);
    update(key, (Number.isFinite(parsed) ? parsed : 0) as Config[typeof key]);
  };

  const save = async () => {
    setSaving(true);
    try {
      await invoke("save_config", { config: cfg });
      setLog((p) => p + "\nSaved. Reloading models…");
      await invoke("reload_models");
      setLog((p) => p + "\nModels reloaded.");
    } catch (e) {
      setLog((p) => p + "\n" + String(e));
    } finally {
      setSaving(false);
    }
  };

  const pickWhisper = async () => {
    const path = await invoke<string | null>("pick_file", { kind: "whisper" });
    if (path) update("whisper_model_path", path);
  };
  const pickLlm = async () => {
    const path = await invoke<string | null>("pick_file", { kind: "llm" });
    if (path) update("llm_model_path", path);
  };

  const issues: { tone: "error" | "warn"; text: string }[] = [];
  if (engine) {
    if (engine.whisper_error) {
      issues.push({ tone: "error", text: `Whisper: ${engine.whisper_error}` });
    } else if (!engine.whisper_loaded) {
      issues.push({
        tone: "warn",
        text: "Whisper model not loaded — dictation will fail until one is configured.",
      });
    }
    if (engine.llm_error) {
      issues.push({ tone: "warn", text: `Gemma: ${engine.llm_error}` });
    }
    if (engine.hotkey_error) {
      issues.push({
        tone: "error",
        text: `Hotkey "${engine.hotkey}" failed to register: ${engine.hotkey_error}. Pick a different combo.`,
      });
    }
  }

  return (
    <div className="settings">
      <h1>OpenWhisper</h1>
      <p className="sub">
        Hold <span className="kbd">{cfg.hotkey}</span> to dictate. Release to
        transcribe, refine with Gemma, and type into the focused app.
      </p>

      {issues.length > 0 && (
        <div className="issues">
          {issues.map((iss, i) => (
            <div key={i} className={`issue issue-${iss.tone}`}>
              {iss.text}
            </div>
          ))}
        </div>
      )}

      <div className="row">
        <label>Global Hotkey</label>
        <input
          type="text"
          value={cfg.hotkey}
          onChange={(e) => update("hotkey", e.target.value)}
          placeholder="e.g. Fn or CmdOrCtrl+Shift+Space"
        />
      </div>

      <div className="row">
        <label>Models</label>
        <div className="models">
          {modelStatus.map((m) => {
            const meta = MODEL_LABELS[m.key];
            const prog = progress[m.key];
            const isDownloading = downloading[m.key];
            const pct =
              prog && prog.total > 0
                ? Math.min(100, Math.round((prog.downloaded / prog.total) * 100))
                : 0;
            return (
              <div key={m.key} className="model">
                <div className="model-head">
                  <div>
                    <div className="model-name">{meta.name}</div>
                    <div className="model-sub">
                      {meta.subtitle}
                      {m.exists ? ` · installed (${formatBytes(m.size)})` : ""}
                    </div>
                  </div>
                  {m.exists ? (
                    <span className="badge ok">Installed</span>
                  ) : isDownloading ? (
                    <span className="badge">
                      {prog && prog.total > 0
                        ? `${pct}%`
                        : prog
                        ? formatBytes(prog.downloaded)
                        : "Starting…"}
                    </span>
                  ) : (
                    <button
                      className="btn secondary"
                      onClick={() => downloadModel(m.key)}
                    >
                      Download
                    </button>
                  )}
                </div>
                {isDownloading && (
                  <div className="bar">
                    <div
                      className="bar-fill"
                      style={{
                        width: prog && prog.total > 0 ? `${pct}%` : "30%",
                      }}
                    />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>

      <div className="row">
        <label>Whisper Model (.bin / GGML)</label>
        <div style={{ display: "flex", gap: 6 }}>
          <input
            type="text"
            value={cfg.whisper_model_path}
            onChange={(e) => update("whisper_model_path", e.target.value)}
            style={{ flex: 1 }}
          />
          <button className="btn secondary" onClick={pickWhisper}>Browse</button>
        </div>
      </div>

      <div className="row">
        <label>Language (whisper)</label>
        <input
          type="text"
          value={cfg.language}
          onChange={(e) => update("language", e.target.value)}
          placeholder="auto / en / es / …"
        />
      </div>

      <div className="row">
        <label>
          <input
            type="checkbox"
            checked={cfg.use_llm_refinement}
            onChange={(e) => update("use_llm_refinement", e.target.checked)}
          />{" "}
          Refine with Gemma
        </label>
      </div>

      <div className="row">
        <label>
          <input
            type="checkbox"
            checked={cfg.fast_paste}
            onChange={(e) => update("fast_paste", e.target.checked)}
          />{" "}
          Fast paste (paste raw transcript immediately, then replace with refined)
        </label>
      </div>

      <div className="row">
        <label>
          <input
            type="checkbox"
            checked={cfg.restore_clipboard}
            onChange={(e) => update("restore_clipboard", e.target.checked)}
          />{" "}
          Restore clipboard after paste (off by default — leaves the dictated text on the clipboard for re-paste)
        </label>
      </div>

      <div className="row">
        <label>Performance</label>
        <div className="perf-grid">
          <div>
            <div className="field-hint">Max recording seconds</div>
            <input
              type="number"
              min={5}
              max={600}
              value={cfg.max_recording_seconds}
              onChange={(e) => updateNumber("max_recording_seconds", e.target.value)}
            />
          </div>
          <div>
            <div className="field-hint">Inference threads (0 = auto)</div>
            <input
              type="number"
              min={0}
              max={32}
              value={cfg.inference_threads}
              onChange={(e) => updateNumber("inference_threads", e.target.value)}
            />
          </div>
        </div>
      </div>

      <div className="row">
        <label>Gemma Model (.gguf)</label>
        <div style={{ display: "flex", gap: 6 }}>
          <input
            type="text"
            value={cfg.llm_model_path}
            onChange={(e) => update("llm_model_path", e.target.value)}
            style={{ flex: 1 }}
          />
          <button className="btn secondary" onClick={pickLlm}>Browse</button>
        </div>
      </div>

      <div className="row">
        <label>Refinement Prompt</label>
        <textarea
          value={cfg.refine_prompt}
          onChange={(e) => update("refine_prompt", e.target.value)}
        />
      </div>

      <div className="actions">
        <button className="btn" disabled={saving} onClick={save}>
          {saving ? "Saving…" : "Save & Reload"}
        </button>
        <button
          className="btn secondary"
          onClick={async () => {
            setLog((p) => p + "\nRecording 3s…");
            try {
              const out = await invoke<string>("test_dictate", {
                seconds: 3,
                inject: false,
              });
              setLog((p) => p + "\nResult: " + out);
            } catch (e) {
              setLog((p) => p + "\nError: " + String(e));
            }
          }}
        >
          Test (record 3s)
        </button>
      </div>

      {log && <pre className="status-line">{log.trim()}</pre>}
    </div>
  );
}
