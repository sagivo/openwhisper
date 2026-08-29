import { useEffect, useState } from "react";
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

  useEffect(() => {
    const onHash = () => setRoute(routeFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  if (route === "about") return <About />;
  if (route === "help") return <Help />;
  if (route === "setup") return <Setup />;
  return <Settings />;
}
