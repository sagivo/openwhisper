#!/usr/bin/env node
// Wipe OpenWhisper user data and start a first-run install.
import { spawn } from "node:child_process";
import { rm } from "node:fs/promises";
import { homedir, platform } from "node:os";
import { join } from "node:path";

function userDataDirs() {
  const home = homedir();
  switch (platform()) {
    case "darwin":
      return [join(home, "Library", "Application Support", "openwhisper")];
    case "win32":
      return [
        join(
          process.env.APPDATA || join(home, "AppData", "Roaming"),
          "openwhisper"
        ),
      ];
    default:
      return [
        join(process.env.XDG_CONFIG_HOME || join(home, ".config"), "openwhisper"),
        join(
          process.env.XDG_DATA_HOME || join(home, ".local", "share"),
          "openwhisper"
        ),
      ];
  }
}

function killRunningApp() {
  const cmd =
    platform() === "win32"
      ? spawn("taskkill", ["/IM", "OpenWhisper.exe", "/F"], { stdio: "ignore" })
      : spawn("killall", ["OpenWhisper"], { stdio: "ignore" });
  return new Promise((resolve) => {
    cmd.on("exit", () => resolve());
    cmd.on("error", () => resolve());
  });
}

await killRunningApp();

for (const dir of userDataDirs()) {
  await rm(dir, { recursive: true, force: true });
  console.log(`cleared ${dir}`);
}

console.log("starting first-run setup…");
const extra = process.argv.slice(2);
const child = spawn("npm", ["run", "tauri", "--", "dev", ...extra], {
  stdio: "inherit",
  shell: true,
});
child.on("exit", (code) => process.exit(code ?? 0));
child.on("error", (err) => {
  console.error(err);
  process.exit(1);
});
