import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

async function boot() {
  if (!isTauri()) {
    const { installBrowserMocks } = await import("./browserDev");
    installBrowserMocks();
  }

  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>
  );
}

void boot();
