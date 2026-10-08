// Baut die MCP-Brücke (remember-key-mcp.exe) und legt sie dort ab, wo Tauri sie als
// `externalBin` erwartet: src-tauri/binaries/remember-key-mcp-<target-triple>.exe.
// Tauri kopiert sie beim Bauen neben die App (Dev und Installer).
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, renameSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const tauriDir = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri");
const triple = /host: (\S+)/.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))[1];

// Die Brücke in target/<profil>/ läuft oft noch (jede Claude-Sitzung startet eine eigene)
// und ist dann gesperrt – Cargo und Tauri könnten sie nicht überschreiben. Eine laufende
// .exe lässt sich unter Windows aber umbenennen. Jede alte Kopie bekommt einen eigenen
// Namen; nicht mehr gesperrte Kopien werden aufgeräumt.
const BRIDGE = "remember-key-mcp.exe";
for (const profile of ["debug", "release"]) {
  const dir = join(tauriDir, "target", profile);
  if (!existsSync(dir)) continue;
  for (const name of readdirSync(dir)) {
    if (name.startsWith(`${BRIDGE}.`) && name.endsWith(".old")) {
      try {
        rmSync(join(dir, name), { force: true });
      } catch {} // läuft noch – beim nächsten Build erneut versuchen
    }
  }
  const exe = join(dir, BRIDGE);
  if (existsSync(exe)) {
    try {
      renameSync(exe, `${exe}.${Date.now()}.old`);
    } catch {}
  }
}

execFileSync("cargo", ["build", "--release", "-p", "rk-agent", "--bin", "remember-key-mcp"], {
  cwd: tauriDir,
  stdio: "inherit",
});
mkdirSync(join(tauriDir, "binaries"), { recursive: true });
copyFileSync(
  join(tauriDir, "target", "release", BRIDGE),
  join(tauriDir, "binaries", `remember-key-mcp-${triple}.exe`),
);
