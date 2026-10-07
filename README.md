# Remember Key

Lokaler Passwort- und Login-Manager für Windows (Tauri 2 · Rust · SvelteKit).

## Entwicklung

```bash
npm install
npm run tauri dev      # App im Dev-Modus starten
npm run check          # TypeScript/Svelte prüfen
cd src-tauri && cargo test --workspace   # Rust-Unit-Tests (Krypto, TOTP, Generator, KI-Zugriff)
npm run tauri build    # Installer (NSIS/MSI) unter src-tauri/target/release/bundle
```

## Design

Oberfläche im Designsystem **Nocturne** (`design/Nocturne/`). Die App nutzt eine Kopie des Stylesheets unter
`src/lib/nocturne/styles.css` (ohne Google-Fonts-Import); Inter und Phosphor-Icons sind lokal eingebunden
(`@fontsource-variable/inter`, `@phosphor-icons/core`). App-eigene Ergänzungen stehen in `src/app.css`.
Nach Änderungen am Designsystem `styles.css` neu kopieren.

## Sicherheitsmodell

- **Tresor-Datei:** `%APPDATA%\com.rememberkey.app\vault.rk` (+ `vault.rk.bak` mit der vorherigen Version).
- **Master-Passwort:** mindestens 10 Zeichen und zxcvbn-Score ≥ 3 (inkl. häufiger deutscher Wörter),
  im Backend erzwungen (`strength.rs`).
- **Schlüsselableitung:** Argon2id, 256 MiB, 3 Iterationen, 16-Byte-Zufalls-Salt. Ältere Tresore werden beim
  Entsperren automatisch auf diese Parameter umgestellt.
- **Verschlüsselung:** XChaCha20-Poly1305, neue 24-Byte-Nonce bei jedem Speichern; der Header
  (Format, KDF-Parameter, Salt, Nonce) ist als AAD authentifiziert. Format siehe `src-tauri/src/vault.rs`.
- **Schlüssel und Klartext** liegen nur im Rust-Backend. Der Schlüssel liegt in per `VirtualLock` gesperrtem
  Speicher (keine Auslagerung), Klartext-JSON wird in Puffer exakter Größe serialisiert; alles wird beim
  Sperren/Beenden genullt. Passwörter gehen nur bei „Anzeigen“/„Bearbeiten“ an die Oberfläche (20 s sichtbar).
- **Passwortwechsel:** löscht lokale Sicherungen mit dem alten Schlüssel; in Google Drive wird die alte Datei
  samt Versionsverlauf durch eine neue ersetzt.
- **Sperre** nach Inaktivität (Standard 5 min), beim Sperren von Windows, im Energiesparmodus und mit Strg+L.
- **Zwischenablage:** Kopieren läuft im Backend; Inhalte werden vom Windows-Verlauf (Win+V) und Cloud-Sync
  ausgeschlossen und nach 30 s geleert (nur, falls zwischenzeitlich nichts anderes kopiert wurde).
- **Webview:** strikte CSP, keine Plugins außer Events, eingefrorene Prototypen.

## KI-Zugriff (MCP)

- KI-Assistenten (z. B. Claude Code) können freigegebene Einträge über die MCP-Brücke `remember-key-mcp.exe`
  nutzen, ohne Passwörter, Notizen oder 2FA-Schlüssel zu sehen. Plan und Bedrohungsmodell: `docs/plan-mcp-server.md`.
- Aus per Standard: globaler Schalter in den Einstellungen, zusätzlich Freigabe pro Eintrag mit gebundenen Hosts
  (Standard: Host der Website).
- Die Brücke (`src-tauri/agent/`) hält keine Geheimnisse und spricht über die Named Pipe
  `\\.\pipe\com.rememberkey.agent.<SID>` (nur aktueller Benutzer) mit der App. Die App akzeptiert nur die Brücke aus
  ihrem eigenen Verzeichnis. Jede Anfrage steht im Protokoll `%APPDATA%\com.rememberkey.app\agent-log.jsonl`.
- `npm run build:bridge` baut die Brücke nach `src-tauri/binaries/` (läuft automatisch vor `tauri dev`/`build`).
- Einrichten: `claude mcp add remember-key -- "<App-Verzeichnis>\remember-key-mcp.exe"`.
- Phase 1: nur `list_entries` (Metadaten). Das Einsetzen von Zugangsdaten folgt in Phase 2.

## Google-Drive-Sync

- Speicherort: versteckter App-Ordner (`appDataFolder`), Berechtigung nur `drive.appdata`. Hochgeladen wird
  ausschließlich die verschlüsselte `vault.rk`.
- Anmeldung: OAuth 2.0 (Desktop-Client, Loopback-Redirect, PKCE). Das Refresh-Token liegt in der
  Windows-Anmeldeinformationsverwaltung (`com.rememberkey.app`), Client-ID/Status in `sync.json`.
- Abgleich nach jeder Änderung (entprellt), beim Entsperren und alle 60 s. Zusammenführung pro Eintrag/Ordner
  (neuere Änderung gewinnt, Löschvermerke verhindern Wiederauferstehung), siehe `VaultData::merge`.
- Passwortwechsel auf einem anderen Gerät: Der Abgleich pausiert, bis das (neue) Master-Passwort bestätigt wird.
- Einrichtung: Google-Cloud-Projekt → Drive API aktivieren → OAuth-Zustimmungsbildschirm (Extern, Status
  „In Produktion“, sonst laufen Tokens nach 7 Tagen ab) → OAuth-Client-ID vom Typ „Desktop-App“.

Das Master-Passwort ist nicht wiederherstellbar. Für Backups einfach `vault.rk` sichern.
