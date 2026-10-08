// Baut die MCP-Brücke (remember-key-mcp.exe) und den Native-Messaging-Host für die
// Browser-Erweiterung (remember-key-browser.exe) und legt sie dort ab, wo Tauri sie als
// `externalBin` erwartet: src-tauri/binaries/<name>-<target-triple>.exe.
// Tauri kopiert sie beim Bauen neben die App (Dev und Installer).
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, renameSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const tauriDir = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri");
const triple = /host: (\S+)/.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))[1];

// Die Programme in target/<profil>/ laufen oft noch (jede Claude-Sitzung startet eine eigene
// Brücke, jeder Browser einen Host) und sind dann gesperrt – Cargo und Tauri könnten sie
// nicht überschreiben. Eine laufende .exe lässt sich unter Windows aber umbenennen. Jede
// alte Kopie bekommt einen eigenen Namen; nicht mehr gesperrte Kopien werden aufgeräumt.
const BINS = ["remember-key-mcp", "remember-key-browser"];
for (const profile of ["debug", "release"]) {
  const dir = join(tauriDir, "target", profile);
  if (!existsSync(dir)) continue;
  for (const bin of BINS) {
    const exeName = `${bin}.exe`;
    for (const name of readdirSync(dir)) {
      if (name.startsWith(`${exeName}.`) && name.endsWith(".old")) {
        try {
          rmSync(join(dir, name), { force: true });
        } catch {} // läuft noch – beim nächsten Build erneut versuchen
      }
    }
    const exe = join(dir, exeName);
    if (existsSync(exe)) {
      try {
        renameSync(exe, `${exe}.${Date.now()}.old`);
      } catch {}
    }
  }
}

execFileSync("cargo", ["build", "--release", "-p", "rk-agent", ...BINS.flatMap((b) => ["--bin", b])], {
  cwd: tauriDir,
  stdio: "inherit",
});
mkdirSync(join(tauriDir, "binaries"), { recursive: true });
for (const bin of BINS) {
  copyFileSync(join(tauriDir, "target", "release", `${bin}.exe`), join(tauriDir, "binaries", `${bin}-${triple}.exe`));
}

// Selbst neben die App legen: Tauri kopiert externalBin nur, wenn Cargo das Build-Skript
// erneut ausführt – sonst fehlten die oben umbenannten Programme danach.
for (const profile of ["debug", "release"]) {
  const dir = join(tauriDir, "target", profile);
  if (!existsSync(dir)) continue;
  for (const bin of BINS) {
    const target = join(dir, `${bin}.exe`);
    if (profile === "release" && existsSync(target)) continue; // ist schon das frische Build-Ergebnis
    try {
      copyFileSync(join(tauriDir, "target", "release", `${bin}.exe`), target);
    } catch (e) {
      console.warn(`${target} konnte nicht aktualisiert werden: ${e.message}`);
    }
  }
}
