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
    pub hosts: Vec<String>,          // exakt oder "*.example.com"; Standard: Host aus `url`
    pub actions: Vec<AgentAction>,   // HttpAuth, FillLogin, (später) Command
    pub http_locations: Vec<HttpLocation>, // AuthorizationBearer, BasicAuth, Header(name), FormField(name)
    pub approval: Approval,          // Always | Session { minutes: u32 }
}
```

`Settings` erhält `agent_enabled: bool` (Standard `false`). Das Protokoll liegt in einer eigenen Datei
`agent-log.jsonl` im App-Verzeichnis. Es enthält nur Metadaten: Zeit, Tool, Eintrags-ID und Titel,
Host, Entscheidung und Client. Die Datei wird auf 1000 Zeilen begrenzt.

## 4. MCP-Tools

| Tool | Eingabe | Rückgabe an KI | Geheimnis wird … |
|---|---|---|---|
| `list_entries` | `query?` | `id`, `title`, `username`, `host`, `actions`, `has_totp` (nur freigegebene Einträge) | nicht berührt |
| `http_request` | `method`, `url`, `headers`, `body?`, `auth: {entry_id, location}` | Status, Header und Body, bereinigt | vom Backend in den Request gesetzt (ureq) |
| `fill_login` (Phase 3) | `entry_id` | `ok` / Fehler | vom Backend ins Zielfenster getippt |

Regeln für `http_request`:

- **Host-Prüfung** nach Normalisierung: Punycode, Kleinschreibung, Port, nur `https`.
- Redirects folgt das Backend nicht automatisch. Jeder `Location`-Sprung wird erneut geprüft, und bei
  einem Hostwechsel entfällt die Authentifizierung.
- Das Geheimnis kommt **nur** an die in `http_locations` erlaubten Stellen. Platzhalter im Body oder in
  der URL gibt es nicht. So kann die KI das Passwort nicht z. B. als Gist-Inhalt an den erlaubten Host
  schicken.
- **Bereinigung der Antwort:** Klartext sowie Base64-, URL- und Hex-Varianten des Geheimnisses werden
  durch `***` ersetzt.
- Header wie `Authorization` und `Set-Cookie` werden aus der Antwort entfernt.
- Größenlimit für Antworten: 1 MiB.
- Geheimnisse liegen nur in `Zeroizing`-Puffern und werden nach dem Request genullt.

TOTP: Eine Location `TotpHeader(name)` oder `FormField` mit Quelle `totp` reicht. Der Code wird wie ein
Passwort behandelt und taucht nie in der Antwort auf.

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
   - Workspace-Crate `mcp-bridge` mit `rmcp`.
   - Pipe-Server in `agent.rs` mit ACL und Client-Prüfung.
   - Globaler Schalter, `AgentPolicy` im Datenmodell inkl. Sync-Merge, Protokoll.
   - `list_entries`.
2. **`http_request`**
   - Host- und Location-Policy, Redirect-Behandlung, Bereinigung.
   - Bestätigungsdialog, UI im Eintrag.
3. **`fill_login`** (Entscheidung offen, siehe unten).
4. **Härtung**
   - Windows Hello, Session-Freigaben.
   - Optional: Befehlsvorlagen (`run_with_secret`, nur vom Nutzer angelegte Vorlagen).
   - Optional: Remember Key als Git-Credential-Helper.

## 8. Tests

- **Rust-Unit-Tests:**
  - Host-Matching (Wildcard, Punycode, Port, `http` abgelehnt)
  - Location-Durchsetzung
  - Bereinigung (alle Kodierungen, Geheimnis über Chunk-Grenzen)
  - Redirect-Hostwechsel
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
| KI steuert Browser und liest Feld per JS aus | `fill_login` erst mit Mechanismus außerhalb der KI-Reichweite (Phase 3) |

## 10. Offene Entscheidungen

1. **Mechanismus für `fill_login`:**
   - **Auto-Type:** `SendInput` ins Vordergrundfenster, Ziel-URL per UI Automation prüfen. Wenig
     Aufwand, aber fragil, und ein KI-gesteuerter Browser kann das Feld danach auslesen.
   - **Browser-Erweiterung mit Native Messaging:** Robuste Origin-Bindung, aber deutlich mehr Aufwand.
   - Empfehlung: zuerst Phase 2 liefern und erst dann entscheiden.
2. **Bridge als Workspace-Crate oder `[[bin]]` im bestehenden Paket.** Empfehlung: eigene Crate, weil
   kleiner und ohne Tresorcode.
3. **Session-Freigaben:** ganz zulassen oder nur „Einmal erlauben“.
