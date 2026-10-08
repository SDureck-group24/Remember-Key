// Erzeugt eine signierte Fassung der Browser-Erweiterung für Firefox/Zen (.xpi), die sich
// dauerhaft installieren lässt – ohne Signatur geht das nur temporär über about:debugging.
//
// Signiert wird „unlisted“ über addons.mozilla.org: Die Erweiterung erscheint nicht im
// öffentlichen Katalog, Mozilla prüft und signiert sie automatisch.
//
// Voraussetzung: API-Schlüssel von https://addons.mozilla.org/developers/addon/api/key/
//   WEB_EXT_API_KEY=…  WEB_EXT_API_SECRET=…  npm run sign:firefox
//
// Ohne Schlüssel (oder mit --no-sign) wird nur der Ordner dist/extension-firefox erzeugt.
// Mit --pack entsteht dist/remember-key-<version>.xpi zum Hochladen auf addons.mozilla.org
// (Entwickler-Hub → „Neues Add-on einreichen“); Mozilla prüft und signiert sie dann.
// Vor jedem neuen Signieren die Version in extension/manifest.json erhöhen.
import { execSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const src = join(root, "extension");
const dist = join(root, "dist");
const out = join(dist, "extension-firefox");

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
cpSync(src, out, { recursive: true });

// Felder entfernen, die nur Chrome kennt (sonst Warnungen bzw. Fehler bei der Prüfung):
// `key` bestimmt die Chrome-ID, `service_worker` ersetzt in Firefox das Hintergrundskript.
const manifestPath = join(out, "manifest.json");
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
delete manifest.key;
delete manifest.background.service_worker;
writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");
console.log(`Firefox-Fassung ${manifest.version} in ${out}`);

const { WEB_EXT_API_KEY, WEB_EXT_API_SECRET } = process.env;
if (process.argv.includes("--no-sign")) process.exit(0);
if (process.argv.includes("--pack")) {
  // Prüfen (Fehler brechen ab) und als .xpi verpacken – Pfade im Archiv mit „/“, wie AMO es verlangt.
  execSync(`npx --yes web-ext@8 lint --source-dir "${out}"`, { stdio: "inherit" });
  const file = `remember-key-${manifest.version}.xpi`;
  execSync(
    `npx --yes web-ext@8 build --source-dir "${out}" --artifacts-dir "${dist}" --filename "${file}" --overwrite-dest`,
    { stdio: "inherit" },
  );
  console.log(`Zum Hochladen: ${join(dist, file)}`);
  process.exit(0);
}
if (!WEB_EXT_API_KEY || !WEB_EXT_API_SECRET) {
  console.error(
    "Zum Signieren WEB_EXT_API_KEY und WEB_EXT_API_SECRET setzen " +
      "(https://addons.mozilla.org/developers/addon/api/key/).",
  );
  process.exit(1);
}

// web-ext liest die Schlüssel aus den Umgebungsvariablen; sie landen nicht in der Kommandozeile.
// Über die Shell, weil npx unter Windows eine .cmd-Datei ist; Pfade daher in Anführungszeichen.
execSync(`npx --yes web-ext@8 sign --channel=unlisted --source-dir "${out}" --artifacts-dir "${dist}"`, {
  stdio: "inherit",
});
if (existsSync(dist)) {
  console.log(`Signierte .xpi liegt in ${dist}. In Zen: about:addons → Zahnrad → „Add-on aus Datei installieren“.`);
}
