import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface VersionInfo {
  current: string;
  latest: string | null;
  update_available: boolean;
}

export default function About() {
  const [info, setInfo] = useState<VersionInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [message, setMessage] = useState<string>("");

  useEffect(() => {
    invoke<VersionInfo>("app_version").then(setInfo).catch((e) => setMessage(String(e)));
  }, []);

  const check = async () => {
    setChecking(true);
    setMessage("Checking for updates…");
    try {
      const next = await invoke<VersionInfo>("check_for_updates");
      setInfo(next);
      if (next.update_available && next.latest) {
        setMessage(`Update available: v${next.latest}`);
      } else {
        setMessage("You're on the latest version.");
      }
    } catch (e) {
      setMessage("Update check failed: " + String(e));
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="settings about">
      <img className="app-logo" src="/icon.svg" width="64" height="64" alt="OpenWhisper" />
      <h1>OpenWhisper</h1>
      <p className="sub">Local-first voice dictation, powered by Whisper.</p>

      <div className="about-block">
        <div className="about-row">
          <span className="about-label">Version</span>
          <span className="about-value">{info ? `v${info.current}` : "…"}</span>
        </div>
        <div className="about-row">
          <span className="about-label">Latest</span>
          <span className="about-value">
            {info?.latest ? `v${info.latest}` : "—"}
          </span>
        </div>
      </div>

      <div className="actions">
        <button className="btn" disabled={checking} onClick={check}>
          {checking ? "Checking…" : "Check for Updates"}
        </button>
      </div>

      {message && <pre className="status-line">{message}</pre>}

      <p className="about-footer">
        © OpenWhisper · MIT
      </p>
    </div>
  );
}
