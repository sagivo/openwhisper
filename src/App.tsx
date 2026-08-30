import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Settings from "./Settings";
import About from "./About";
import Help from "./Help";
import Setup from "./Setup";

type Route = "settings" | "about" | "help" | "setup";

function routeFromHash(): Route {
  const h = window.location.hash.replace(/^#\/?/, "").toLowerCase();
  if (h === "about") return "about";
  if (h === "help") return "help";
  if (h === "setup") return "setup";
  return "settings";
}

export default function App() {
  const [route, setRoute] = useState<Route>(routeFromHash);
  const [updateVersion, setUpdateVersion] = useState<string | null>(null);

  useEffect(() => {
    const onHash = () => setRoute(routeFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  useEffect(() => {
    invoke<string | null>("pending_update").then(setUpdateVersion).catch(() => {});
    const un = listen<string>("update-ready", (e) => setUpdateVersion(e.payload));
    return () => {
      un.then((fn) => fn());
    };
  }, []);

  const page =
    route === "about" ? (
      <About />
    ) : route === "help" ? (
      <Help />
    ) : route === "setup" ? (
      <Setup />
    ) : (
      <Settings />
    );

  return (
    <div className="app-shell">
      {updateVersion && (
        <div className="update-banner" role="status">
          <span>Version {updateVersion} is ready. Restart to install it.</span>
          <button className="btn small" onClick={() => invoke("relaunch_app")}>
            Restart
          </button>
        </div>
      )}
      <div className="app-shell-body">{page}</div>
    </div>
  );
}
