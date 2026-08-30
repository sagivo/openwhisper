import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import { BRAND_EMOJI } from "./brand";

const COFFEE_URL = "https://buymeacoffee.com/sagivo";

interface VersionInfo {
  current: string;
  latest: string | null;
  update_available: boolean;
  ready: boolean;
}

function updateStatusMessage(info: VersionInfo): string {
  if (info.ready && info.latest) {
    return `v${info.latest} downloaded. Restart to install.`;
  }
  if (info.update_available && info.latest) {
    return `Update available: v${info.latest}`;
  }
  if (info.latest) {
    return "You're on the latest version.";
  }
  return "Couldn't determine the latest version.";
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
      setMessage(updateStatusMessage(next));
    } catch (e) {
      setMessage("Update check failed: " + String(e));
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="settings about">
      <div className="app-logo" aria-hidden="true">{BRAND_EMOJI}</div>
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

      <button
        className="coffee-link"
        onClick={() => open(COFFEE_URL).catch(() => window.open(COFFEE_URL, "_blank"))}
      >
        ☕ Buy me a coffee
      </button>

      <p className="about-footer">
        © OpenWhisper · MIT
      </p>
    </div>
  );
}
