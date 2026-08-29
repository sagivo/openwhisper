import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Config {
  hotkey: string;
  whisper_model_path: string;
  llm_model_path: string;
  refine_prompt: string;
  use_llm_refinement: boolean;
  language: string;
}

export default function Help() {
  const [hotkey, setHotkey] = useState<string>("Fn");

  useEffect(() => {
    invoke<Config>("get_config")
      .then((c) => setHotkey(c.hotkey || "Fn"))
      .catch(() => {});
  }, []);

  return (
    <div className="settings help">
      <h1>How to use OpenWhisper</h1>
      <p className="sub">
        OpenWhisper lives in your menu bar and turns your voice into text in
        any app — fully on-device, no cloud.
      </p>

      <h2>Getting started</h2>
      <ol className="help-list">
        <li>
          Open OpenWhisper. The first launch downloads the speech and local AI
          models and asks for Microphone and Accessibility — no Settings trip
          required.
        </li>
        <li>
          When the window says you're ready, click into any text field, hold{" "}
          <span className="kbd">{hotkey}</span>, and speak.
        </li>
      </ol>

      <h2>Dictating</h2>
      <ol className="help-list">
        <li>
          <strong>Hold</strong> <span className="kbd">{hotkey}</span> and
          start speaking. The menu bar shows 🤫 while you dictate.
        </li>
        <li>
          <strong>Release</strong> the hotkey when you're done. OpenWhisper
          transcribes locally with Whisper, then optionally cleans up the
          text with a local AI model.
        </li>
        <li>
          The result is <strong>automatically pasted</strong> into the
          focused text field. If no text field is focused, it lands on your{" "}
          <strong>clipboard</strong> — just press{" "}
          <span className="kbd">Cmd+V</span>.
        </li>
      </ol>

      <h2>Menu bar</h2>
      <ul className="help-list">
        <li>
          Click the menu bar icon to open the menu:{" "}
          <em>Start / Stop Dictation</em>, <em>Open Settings…</em>,{" "}
          <em>About</em>, <em>Help</em>, and <em>Quit</em>.
        </li>
        <li>
          Use <em>Start / Stop Dictation</em> as a hands-free alternative to
          the hotkey — great for long sessions.
        </li>
        <li>
          The 🤫 tooltip reflects the current state (recording,
          transcribing, refining, pasting, or error).
        </li>
      </ul>

      <h2>Settings</h2>
      <ul className="help-list">
        <li>
          <strong>Global Hotkey</strong> — default{" "}
          <span className="kbd">Fn</span>, or any combination of Cmd, Ctrl,
          Alt, Shift plus a key (e.g.{" "}
          <span className="kbd">Cmd+Shift+Space</span>). Use{" "}
          <span className="kbd">CmdOrCtrl</span> for cross-platform configs.
        </li>
        <li>
          <strong>Language</strong> — set a Whisper language code (e.g.{" "}
          <code>en</code>, <code>es</code>) or <code>auto</code> for
          detection.
        </li>
        <li>
          <strong>Refine with local AI</strong> — runs a small local LLM over
          the transcript to clean up filler words and punctuation. You can
          tweak the <em>Refinement Prompt</em> to match your style.
        </li>
        <li>
          <strong>Test (record 3s)</strong> — captures three seconds from the
          mic and shows the result in the log without pasting. Handy for
          verifying mic, models, and language settings.
        </li>
      </ul>

      <h2>Tips & troubleshooting</h2>
      <ul className="help-list">
        <li>
          <strong>Nothing happens on the hotkey?</strong> Make sure
          OpenWhisper has <em>Accessibility</em> and, for{" "}
          <span className="kbd">Fn</span>, <em>Input Monitoring</em> in{" "}
          <em>System Settings → Privacy & Security</em>, and that no
          other app is using the same shortcut.
        </li>
        <li>
          <strong>Pasting doesn't work in a particular app?</strong> The
          transcript is still on your clipboard — just press{" "}
          <span className="kbd">Cmd+V</span>.
        </li>
        <li>
          <strong>Silent or empty recordings</strong> are detected and
          skipped automatically, so you won't get stray "[BLANK_AUDIO]" text
          pasted.
        </li>
        <li>
          <strong>Everything runs locally.</strong> Audio, transcripts, and
          refinement never leave your machine.
        </li>
      </ul>
    </div>
  );
}
