import { useEffect, useState } from "react";
import Settings from "./Settings";
import About from "./About";
import Help from "./Help";

type Route = "settings" | "about" | "help";

function routeFromHash(): Route {
  const h = window.location.hash.replace(/^#\/?/, "").toLowerCase();
  if (h === "about") return "about";
  if (h === "help") return "help";
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
  return <Settings />;
}
