# Plan: MCP-Server für Remember Key („KI-Zugriff ohne Einsicht“)

Ziel: Ein KI-Chat (Claude Code, Claude Desktop, andere MCP-Clients) kann Zugangsdaten aus Remember Key
**benutzen**, ohne sie je zu **sehen**. Die KI arbeitet nur mit Verweisen auf Einträge. Das Rust-Backend
setzt das Passwort selbst am Ziel ein, nach Regeln pro Eintrag und nach Bestätigung durch den Nutzer.

## 1. Grundsätze

1. **Kein Geheimnis im Kontext der KI.** Kein Tool gibt Passwort, Notizen, TOTP-Secret oder TOTP-Code
   zurück. Antworten enthalten nur Metadaten und Ergebnisse, die von Geheimnissen bereinigt sind.
2. **Opt-in auf zwei Ebenen:** Ein globaler Schalter „KI-Zugriff“ ist standardmäßig aus. Zusätzlich
   wird jeder Eintrag einzeln freigegeben.
3. **Bindung an Ziel und Ort:** Ein Geheimnis geht nur an hinterlegte Hosts. Dort landet es nur an
   hinterlegten Stellen, z. B. im `Authorization`-Header, aber nicht im frei wählbaren Body.
4. **Mensch bestätigt:** Jede Nutzung eines Geheimnisses erzeugt einen Dialog in der App. Ohne Antwort
   gilt nach 60 s „Ablehnen“.
5. **Nachvollziehbarkeit:** Jede Anfrage landet im Protokoll, auch abgelehnte.
6. **Die Brücke kennt nichts:** Der MCP-Prozess hält keine Schlüssel und keine Klartexte. Alles
   Sensible bleibt im laufenden Tauri-Backend.

## 2. Architektur

```
MCP-Client (Claude)  ──stdio/JSON-RPC──▶  remember-key-mcp.exe  ──Named Pipe──▶  Remember Key (Tauri)
                                          (dünne Brücke, ohne                     agent.rs: Policy,
                                           Tresorzugriff)                         Bestätigung, Einsetzen,
                                                                                  Bereinigung, Protokoll
```

- **`remember-key-mcp.exe`** (Workspace-Crate `src-tauri/agent/`, Paket `rk-agent`)
  - Rust, MCP über stdio (zeilenweises JSON-RPC) selbst implementiert statt `rmcp`: kein Async-Runtime,
    ca. 250 KB, leicht prüfbar.
  - Die Crate enthält auch das Pipe-Protokoll (Bibliothek `rk_agent`), das die App mitnutzt.
  - Bewusst ohne Abhängigkeit von `remember_key_lib`, damit er keinen Krypto- oder Tresorcode enthält.
  - Er übersetzt MCP-Tool-Aufrufe 1:1 in Pipe-Nachrichten (JSON, längenpräfixiert).
  - Läuft die App nicht, antwortet er mit „Remember Key ist nicht gestartet“.
  - Wird mit der App installiert (Tauri `bundle.externalBin`).
- **Named Pipe** `\\.\pipe\com.rememberkey.agent.<Benutzer-SID>`
  - Die DACL lässt nur den aktuellen Benutzer zu. `PIPE_REJECT_REMOTE_CLIENTS` ist gesetzt.
  - Die App prüft den Client per `GetNamedPipeClientProcessId` und `QueryFullProcessImageNameW`. Nur
    `remember-key-mcp.exe` aus dem Installationsverzeichnis wird akzeptiert.
  - Prozessname und Elternprozess, also welcher KI-Client, erscheinen im Bestätigungsdialog.
  - Die Pipe ist immer offen, weil der globale Schalter im Tresor liegt und erst nach dem Entsperren
    bekannt ist. Bei gesperrtem Tresor antwortet sie mit `locked`, bei ausgeschaltetem KI-Zugriff mit
    `disabled`, in beiden Fällen ohne Daten.
  - Die Brücke prüft umgekehrt per `GetNamedPipeServerProcessId`, dass der Server aus ihrem eigenen
    Verzeichnis stammt. Sie verbindet sich mit `SECURITY_IDENTIFICATION`, damit sich der Server nicht als
    sie ausgeben kann.
- **`src-tauri/src/agent.rs`** (neu)
  - Enthält Pipe-Server-Thread, Policy-Prüfung, Bestätigungsablauf, Einsetzen, Bereinigung und Protokoll.
  - Nutzt `AppState` aus `lib.rs` genauso wie die bestehenden `#[tauri::command]`s.
  - Ein Agent-Zugriff zählt **nicht** als Aktivität (`touch`). Die Inaktivitätssperre bleibt wirksam.

## 3. Datenmodell (vault.rs)

Neues optionales Feld am `Entry`, abwärtskompatibel per `#[serde(default)]` und im Sync über
`updated_at` mitzusammengeführt:

```rust
pub struct AgentPolicy {
    pub enabled: bool,
    pub hosts: Vec<String>,         // exakt oder "*.example.com" (nur Subdomains); Standard: Host aus `url`
    pub auth: Vec<AuthLocation>,    // Bearer | Basic | Header | FormField | JsonField; leer = nur Auflisten
    pub session_minutes: u32,       // 0 = jede Anfrage bestätigen, sonst bis 60 Minuten
}
```

Zusätzlich hat jeder Eintrag ein optionales Feld `api_token`. Ist es gefüllt, setzt `http_request` den
Token statt des Passworts ein, denn die meisten APIs akzeptieren kein Login-Passwort (siehe weclapp in
`docs/beobachtungen-ki-test-2026-10-08.md`). Beide Geheimnisse werden aus Antworten entfernt.
`list_entries` meldet unter `secret`, was eingesetzt würde.

Umgesetzt ist das schlanker als ursprünglich geplant: Statt einer eigenen `actions`-Liste regelt `auth`,
ob und wo das Passwort eingesetzt werden darf. `fill_login` ist ein eigener Schalter.

`Settings` erhält `agent_enabled: bool` (Standard `false`). Das Protokoll liegt in einer eigenen Datei
`agent-log.jsonl` im App-Verzeichnis. Es enthält nur Metadaten: Zeit, Tool, Eintrags-ID und Titel,
Host, Entscheidung und Client. Die Datei wird auf 1000 Zeilen begrenzt.

## 4. MCP-Tools

| Tool | Eingabe | Rückgabe an KI | Geheimnis wird … |
|---|---|---|---|
| `list_entries` | `query?` | `id`, `title`, `username`, `hosts`, `auth`, `hasTotp` (nur freigegebene Einträge) | nicht berührt |
| `http_request` | `entry_id`, `url`, `method?`, `headers?`, `body?`, `auth?`, `session?`, `end_session?` | Status, Header und Body, bereinigt | vom Backend in den Request gesetzt (ureq) |
| `fill_login` | `entry_id`, `url?` | Host, Titel, ausgefüllte Felder, abgesendet | von der Browser-Erweiterung in den passenden Tab gefüllt und sofort abgesendet |

Regeln für `http_request`:

- **Host-Prüfung** nach Normalisierung: Punycode, Kleinschreibung, Port, nur `https`. `http` ist nur für
  `localhost`, `127.0.0.1` und `[::1]` erlaubt, weil dieser Verkehr den Rechner nicht verlässt. Der Host
  muss trotzdem freigegeben sein.
- Die Antwort enthält unter `injected` eine Diagnose ohne Geheimnis (Einsetz-Stelle, Header-Name, bei
  Basic-Auth ob ein Benutzername dabei war). So lässt sich ein 401 durch falschen Token von einem Fehler
  beim Einsetzen unterscheiden.
- Redirects verfolgt das Backend gar nicht. Die KI bekommt Status und `Location` und muss die nächste
  Adresse selbst anfragen. Diese Anfrage wird wie jede andere geprüft und bestätigt.
- Die KI darf keine `Authorization`-, `Cookie`-, `Host`- oder Verbindungs-Header setzen und auch nicht
  den Header der gewählten Einsetz-Stelle. Erlaubt sind die Methoden GET, HEAD, POST, PUT, PATCH und
  DELETE, und GET/HEAD ohne Body.
- Das Geheimnis kommt **nur** an die in `http_locations` erlaubten Stellen. Platzhalter im Body oder in
  der URL gibt es nicht. So kann die KI das Passwort nicht z. B. als Gist-Inhalt an den erlaubten Host
  schicken.
- **Bereinigung der Antwort:** Das Geheimnis wird durch `***` ersetzt, und zwar für das Passwort und
  `Benutzer:Passwort` jeweils als Klartext, Base64 (Standard und URL-sicher, mit und ohne Padding), Hex,
  Prozent- und Formularkodierung, JSON- und HTML-Escaping. Das gilt auch für Fehlermeldungen.
- Header wie `Authorization` und `Set-Cookie` werden aus der Antwort entfernt.
- **Cookie-Sitzungen** (`session`): Mit `session: "new"` wird das Geheimnis wie gewohnt eingesetzt, und die
  Cookies der Antwort bleiben in einem Cookie-Speicher der App (nur Name und Wert, `Zeroizing`). Die KI
  bekommt nur `session.id` zurück (`s_` + 128 Bit Zufall). Folgeanfragen mit `session: <id>` setzen kein
  Geheimnis ein, nur den `Cookie`-Header. `auth` ist dann verboten. Jede Folgeanfrage wird erneut gegen die
  Freigabe geprüft und fällt unter dieselbe Sitzungsfreigabe wie das Login (Eintrag, Host, Einsetz-Stelle).
  Ohne Sitzungsfreigabe wird jede Anfrage einzeln bestätigt. Die Sitzung ist an den Host des Logins
  gebunden und endet so:
  - mit `end_session: true`
  - wenn der Server alle Cookies löscht
  - nach 15 Minuten ohne Nutzung, spätestens nach einer Stunde
  - beim Sperren
  - wenn die Freigabe nicht mehr passt

  Cookie-Werte ab 8 Zeichen werden wie das Geheimnis aus Antworten entfernt.
- Größenlimit für den Antwort-Body: 512 KiB (danach `truncated: true`). Binärdaten werden nur als Größe
  gemeldet.
- Geheimnisse liegen nur in `Zeroizing`-Puffern und werden nach dem Request genullt. Ausnahme: Kopien,
  die `ureq` intern für den Header anlegt, lassen sich nicht nullen.

Body-Felder (Phase 4): Bei `form:<Name>` und `json:<Name>` ergänzt die App das Feld im Body
(Formular bzw. JSON-Objekt auf oberster Ebene) und setzt den passenden Content-Type. Erlaubt ist das
nur mit POST, PUT oder PATCH. Der Body der KI darf das Feld nicht selbst enthalten.

TOTP: Bei `fill_login` schickt die App den aktuellen Code mit, sofern der Eintrag 2FA hat. Die
Erweiterung setzt ihn nur ein, wenn die Seite kein Passwortfeld, aber ein Code-Feld zeigt
(`autocomplete="one-time-code"`, typische Namen oder einzelne Ziffernfelder). Bei `http_request` gibt
es kein TOTP-Einsetzen.

## 5. Bestätigungsablauf

1. Das Backend erhält die Anfrage, prüft die Policy und lehnt bei Verstoß sofort ab, ohne Dialog.
2. Ein Event `agent-approval` geht ans Frontend. Das Fenster kommt in den Vordergrund und zeigt ein
   Nocturne-Modal mit:
   - Client
   - Eintrag
   - Aktion
   - Zielhost
   - Methode und Pfad
   - Stelle, an der das Geheimnis eingesetzt wird
3. Mögliche Antworten:
   - „Einmal erlauben“
   - „Für N Minuten erlauben“ (nur wenn `Approval::Session`)
   - „Ablehnen“
4. Phase 4: Zusätzlich Windows Hello (`UserConsentVerifier`) als zweiter Faktor für die Freigabe.

Der Dialog ist eine Freigabe im Frontend. Das Geheimnis selbst geht dabei nie an die Oberfläche.

## 6. UI

- **Einstellungen:** Schalter „KI-Zugriff“, Anleitung zum Einrichten mit kopierbarem Befehl, Protokoll-Ansicht.
- **Eintrag bearbeiten:** Abschnitt „KI-Zugriff“ mit Schalter, Hosts, Aktionen, Einsetz-Stellen und
  Freigabemodus.
- **Listenansicht:** Kennzeichnung der Einträge, die für die KI freigegeben sind.

Einrichtung im Client:

```bash
claude mcp add remember-key -- "%LOCALAPPDATA%\Remember Key\remember-key-mcp.exe"
```

## 7. Phasen

1. **Grundgerüst** – umgesetzt
   - Workspace-Crate `src-tauri/agent` mit eigenem MCP-Protokoll.
   - Pipe-Server in `agent.rs` mit ACL und Client-Prüfung.
   - Globaler Schalter, `AgentPolicy` im Datenmodell inkl. Sync-Merge, Protokoll.
   - `list_entries`.
2. **`http_request`** – umgesetzt
   - Host- und Location-Policy, keine Redirects, Bereinigung.
   - Bestätigungsdialog mit Countdown und Sitzungsfreigaben, UI im Eintrag.
   - Nach der Bestätigung wird die Freigabe erneut geprüft, bevor das Passwort gelesen wird.
3. **`fill_login`** per WebExtension (Manifest V3) für Zen/Firefox und Chrome mit Native Messaging zur App –
   umgesetzt. Hintergrund: siehe `docs/beobachtungen-ki-test-2026-10-08.md`, Abschnitt 2.4.
   - Erweiterung in `extension/`. Sie läuft unverändert in Chrome/Edge (Service Worker) und Firefox/Zen
     (Hintergrundskript). Die feste Chrome-ID ergibt sich aus dem `key` im Manifest, die Gecko-ID ist
     `remember-key@rememberkey.app`.
   - Native-Messaging-Host `remember-key-browser.exe` (zweites Programm in `rk-agent`). Er meldet den
     Browser mit `RegisterBrowser` an und hält diese Verbindung für Befehle der App offen. Ergebnisse
     schickt er über eigene, kurze Verbindungen zurück, weil synchrone Pipe-Handles Lesen und Schreiben
     serialisieren.
   - Die App akzeptiert auf der Pipe die Brücke nur mit KI-Anfragen und den Host nur mit
     Browser-Nachrichten.
   - Registrierung unter HKCU für Chrome, Edge und Mozilla (gilt auch für Zen), sobald der KI-Zugriff
     eingeschaltet bzw. der Tresor entsperrt wird. Die Manifeste liegen unter
     `%APPDATA%\com.rememberkey.app\native\`.
   - Eigene Freigabe pro Eintrag (`fill_login`) und eigener Bestätigungsdialog. Die Erweiterung wählt
     den passenden Tab (https bzw. http nur für Loopback, Host der Freigabe, aktiv bzw. zuletzt benutzt)
     und prüft den Host in der Seite noch einmal. Sie füllt nur sichtbare Felder im Hauptframe und
     sendet sofort ab. Zweistufige Logins brauchen pro Schritt einen Aufruf.
   - Grenzen: Login-Formulare in iframes werden nicht ausgefüllt. In Zen/Firefox ist die Erweiterung
     ohne Signatur nur temporär ladbar (`about:debugging`).
4. **Härtung** – umgesetzt
   - Windows Hello: Mit der Einstellung `agentHello` muss jede Zustimmung zusätzlich mit PIN,
     Fingerabdruck oder Gesicht bestätigt werden (`UserConsentVerifier` über dem App-Fenster).
     Scheitert die Prüfung, bleibt die Anfrage offen.
   - Body-Felder `form:` und `json:` sowie 2FA-Codes bei `fill_login` (siehe Abschnitt 4).
   - Signierte Erweiterung für Zen/Firefox: `npm run sign:firefox` erzeugt eine Fassung ohne
     Chrome-Felder und signiert sie unlisted über addons.mozilla.org. Die API-Schlüssel setzt der
     Nutzer selbst als Umgebungsvariablen.
   Weiterhin offen:
   - Optional: Befehlsvorlagen (`run_with_secret`, nur vom Nutzer angelegte Vorlagen).
   - Optional: Remember Key als Git-Credential-Helper.

## 8. Tests

- **Rust-Unit-Tests:**
  - Host-Matching (Wildcard, Punycode, Port, `http` abgelehnt)
  - Location-Durchsetzung
  - Bereinigung (alle Kodierungen); zusätzlich `cargo test -- --ignored echo_service` gegen httpbin.org
    mit Dummy-Passwort für alle drei Einsetz-Stellen
  - Serde-Kompatibilität alter Tresore
  - Sync-Merge von `AgentPolicy`
- **Integrationstest:** Pipe-Client gegen `agent.rs` mit Test-Tresor und lokalem HTTPS-Mock.
- **Manueller Test mit Claude Code:**
  - Freigegebener Eintrag funktioniert.
  - Nicht freigegebener Eintrag, falscher Host und Ablehnen werden abgewiesen.
  - Prompt-Injection-Versuche (Passwort in Body oder URL) werden abgewiesen.

## 9. Bedrohungsmodell und Grenzen

| Angriff | Gegenmaßnahme |
|---|---|
| KI bittet direkt ums Passwort | Kein Tool liefert es. |
| Prompt-Injection schickt Geheimnis an fremden Host | Host-Bindung, Bestätigungsdialog zeigt Ziel |
| Geheimnis im Body an erlaubten Host (z. B. Gist) | Nur konfigurierte Einsetz-Stellen |
| Ziel-API spiegelt Header zurück | Bereinigung der Antwort inkl. Kodierungen |
| Anderer Prozess spricht die Pipe an | DACL nur Benutzer, Prüfung des Client-Pfads, Dialog zeigt Client |
| Malware mit Benutzerrechten | Nicht vollständig abwehrbar (gilt auch heute); Dialog + Hello erschweren Missbrauch |
| KI steuert Browser und liest Feld per JS aus | Nicht vollständig abwehrbar. Die Erweiterung sendet sofort ab, und der Dialog weist auf das Risiko hin. Die Tool-Beschreibung verbietet das Auslesen. |
| Erweiterung füllt auf falscher Seite aus | Tab-Auswahl und Prüfung in der Seite gegen die Hosts der Freigabe, nur https bzw. http für Loopback |
| Fremde Erweiterung spricht den Host an | Native-Messaging-Manifest erlaubt nur unsere Erweiterungs-ID; der Host darf in der App nur Browser-Nachrichten senden |

## 10. Offene Entscheidungen

1. **Mechanismus für `fill_login`:**
   - **Auto-Type:** `SendInput` ins Vordergrundfenster, Ziel-URL per UI Automation prüfen. Wenig
     Aufwand, aber fragil, und ein KI-gesteuerter Browser kann das Feld danach auslesen.
   - **Browser-Erweiterung mit Native Messaging:** Robuste Origin-Bindung, aber deutlich mehr Aufwand.
   - Empfehlung: zuerst Phase 2 liefern und erst dann entscheiden.
   - **Entschieden (08.10.2026):** WebExtension für Zen (Firefox) und Chrome. Grenze bleibt: Steuert die
     KI den Browser und kann Seiten-JavaScript ausführen, kann sie ein ausgefülltes Feld lesen. Die
     Erweiterung soll deshalb ausfüllen und sofort absenden.
2. **Bridge als Workspace-Crate oder `[[bin]]` im bestehenden Paket.** Empfehlung: eigene Crate, weil
   kleiner und ohne Tresorcode.
3. **Session-Freigaben:** ganz zulassen oder nur „Einmal erlauben“.
