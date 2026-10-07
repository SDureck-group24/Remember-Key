// Baut die MCP-Brücke (remember-key-mcp.exe) und legt sie dort ab, wo Tauri sie als
// `externalBin` erwartet: src-tauri/binaries/remember-key-mcp-<target-triple>.exe.
// Tauri kopiert sie beim Bauen neben die App (Dev und Installer).
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const tauriDir = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri");
const triple = /host: (\S+)/.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))[1];

execFileSync("cargo", ["build", "--release", "-p", "rk-agent", "--bin", "remember-key-mcp"], {
  cwd: tauriDir,
  stdio: "inherit",
});
mkdirSync(join(tauriDir, "binaries"), { recursive: true });
copyFileSync(
  join(tauriDir, "target", "release", "remember-key-mcp.exe"),
  join(tauriDir, "binaries", `remember-key-mcp-${triple}.exe`),
);
