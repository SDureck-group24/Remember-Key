# Beobachtungen: Test des MCP-Servers mit Claude Code (07./08.10.2026)

Testlauf aus Sicht der KI (Claude Code im Claude-Desktop-App, Code-Tab). Benutzt wurden die Tools
`list_entries` und `http_request`. Ergänzt um Beobachtungen zum Browser-Login.

## 1. Ablauf

| # | Aktion | Ergebnis |
|---|---|---|
| 1 | `list_entries` | Fehler „Der KI-Zugriff ist in Remember Key ausgeschaltet (Einstellungen → KI-Zugriff)“ |
| 2 | `list_entries` (nach Einschalten) | 1 Eintrag: `localhost:7781`, Benutzer `admin`, Host `localhost`, kein `auth`-Feld |
| 3 | Login im Browser auf `http://localhost:7781` (Haufe X360) | Benutzername vorausgefüllt, Passwort musste der Nutzer eingeben |
| 4 | `http_request` an `https://localhost:7781/...` | **Von Claude Code blockiert**, nicht von Remember Key (siehe 2.1) |
| 5 | `list_entries` (`weclapp`) | Eintrag „Team weclapp“, Host `team.weclapp.com`, `auth: [bearer]` |
| 6 | `http_request` GET `/webapp/api/v1/user/currentUser`, Bearer | 401 „no identity available“ |
| 7 | dasselbe nach Passwortänderung, Bearer | 401 „no identity available“ |
| 8 | dasselbe, `auth: [basic]` | 401 „no identity available“ |
| 9 | dasselbe, `auth: [header:AuthenticationToken]` | 401 „no identity available“ |
| 10 | Login im Browser auf `team.weclapp.com` | E-Mail vorausgefüllt, Passwort musste der Nutzer eingeben |

## 2. Beobachtungen

### 2.1 Claude Code blockiert `http_request` im Auto-Modus

Der erste Aufruf von `http_request` wurde vom Auto-Modus-Klassifikator von Claude Code abgelehnt
(„Credential Exploration“). Remember Key hat die Anfrage gar nicht erhalten. Spätere Aufrufe gingen durch.
Das ist für den Nutzer nicht von einer Ablehnung durch Remember Key zu unterscheiden.

**Vorschlag:** In der Einrichtungsanleitung (Einstellungen → KI-Zugriff) eine Berechtigungsregel für
`mcp__remember-key__http_request` in den Claude-Code-Einstellungen erwähnen.

### 2.2 Lokale Instanzen über `http` sind nicht nutzbar

`http_request` erlaubt nur `https`. Lokale Entwicklungsinstanzen (hier Haufe X360 auf
`http://localhost:7781`) laufen oft ohne TLS. Der Eintrag `localhost:7781` hatte außerdem kein `auth`-Feld,
war also ohnehin nur zum Auflisten freigegeben. Ob ein `http`-Aufruf sauber mit einer verständlichen Meldung
abgelehnt wird, wurde nicht getestet.

**Vorschlag:** Optional `http` nur für Loopback (`localhost`, `127.0.0.1`, `[::1]`) pro Eintrag erlauben.
Außerdem die Einschränkung „nur https“ in der Tool-Beschreibung von `list_entries` bzw. in der UI des
Eintrags sichtbar machen.

### 2.3 weclapp: 401 bei allen drei Einsetz-Stellen

Alle Versuche endeten mit derselben Antwort `401 no identity available`, unabhängig von Bearer, Basic oder
`header:AuthenticationToken`. Die Ursache ist offen. Mögliche Gründe:

1. Im Eintrag steht das Login-Passwort statt eines weclapp-**API-Tokens**. Die weclapp-API akzeptiert
   kein Login-Passwort.
2. Der Token gehört zu einem anderen Mandanten als `team.weclapp.com`.
3. Remember Key setzt den Header nicht oder nicht wie erwartet ein.

Die KI kann zwischen diesen Fällen nicht unterscheiden, weil sie die gesendete Anfrage nicht sieht. Der
Nutzer hat mehrfach „nochmal“ angefordert. Jede Wiederholung kostete eine Bestätigung, ohne neue
Information zu liefern.

**Vorschläge:**
- Die Antwort von `http_request` um geheimnisfreie Diagnosedaten erweitern, z. B.
  `"injected": {"location": "header:AuthenticationToken", "secretEmpty": false}`. Damit lässt sich
  Ursache 3 ausschließen, ohne etwas über das Geheimnis preiszugeben.
- Im Eintrag deutlich machen, dass bei gesetztem `auth` das Passwortfeld den **API-Token** enthalten muss.
  Alternativ ein eigenes Feld „API-Token“ neben dem Login-Passwort, damit derselbe Eintrag für Browser-Login
  und API taugt.
- Optional einen „Testen“-Knopf im Eintrag, der eine vom Nutzer hinterlegte Prüf-URL aufruft.

### 2.4 Browser-Login fehlt (`fill_login`)

Der Nutzer wollte sich direkt auf der Website statt über die API anmelden lassen. Das ging nicht:

- Remember Key bietet dafür noch kein Tool (Phase 3).
- Claude gibt Passwörter auf echten Websites grundsätzlich nicht selbst ein, auch nicht mit Zustimmung des
  Nutzers. Es hat nur Benutzername bzw. E-Mail vorausgefüllt.

Das bestätigt den Punkt aus dem Bedrohungsmodell: Ein Browser, den die KI steuert, kann ein eingesetztes
Passwortfeld auslesen. `fill_login` braucht deshalb einen Mechanismus außerhalb der Reichweite der KI. Die
Browser-Erweiterung mit Native Messaging passt dazu besser als Auto-Type.

### 2.5 `list_entries` hing nach einem Neustart der App (behoben)

Nach einem Neustart der App blieb `list_entries` zweimal ohne Antwort hängen. Der Aufruf musste
abgebrochen werden. Danach liefen nur noch die Brücken-Prozesse; App-Prozess und Pipe gab es nicht mehr.
Ein Freigabe-Dialog war hier nicht zu erwarten, `list_entries` braucht keine Freigabe.

Ursachen im Code:

1. **Die Brücke hatte kein Zeitlimit.** `send` in `src-tauri/agent/src/main.rs` wartete mit `read_message`
   unbegrenzt auf die Antwort. Hängt die App oder beendet sie sich bei noch offener Pipe, blockiert der
   Tool-Aufruf für immer. Ein Neuverbinden des MCP-Servers hilft nicht, weil die Brücke ohnehin für jeden
   Aufruf eine neue Verbindung öffnet.
2. **Möglicher Deadlock beim abgewiesenen Client.** `serve` in `src-tauri/src/agent.rs` schrieb die
   Fehlermeldung samt `flush`, ohne vorher die Anfrage zu lesen. `FlushFileBuffers` wartet bei Pipes, bis
   die Gegenseite gelesen hat, und der Client stand selbst noch im `flush` seiner Anfrage. Damit warteten
   beide Seiten dauerhaft aufeinander. Das tritt nur auf, wenn die Brücke die App akzeptiert, die App die
   Brücke aber nicht.

Behebung:

- Die Brücke führt den Austausch über die Pipe in einem eigenen Thread aus und wartet höchstens 15 s
  (`list_entries`) bzw. 120 s (`http_request`: 60 s Dialog + 10 s Verbindungsaufbau + 30 s Anfrage +
  Reserve). Danach bricht sie das blockierende I/O per `CancelSynchronousIo` ab und meldet „Remember Key hat
  nicht innerhalb von … s geantwortet“.
- Die App liest bei einem abgewiesenen Client zuerst die Anfrage und schreibt erst dann die Fehlermeldung.

**Hinweis:** Laufende Brücken-Prozesse nutzen noch die alte `remember-key-mcp.exe`. Wirksam wird die
Behebung erst nach einem neuen Build und einem Neustart der MCP-Sitzung im Client.

### 2.6 Was gut funktioniert hat

- Die Meldung bei ausgeschaltetem KI-Zugriff ist klar und nennt den Ort der Einstellung.
- Änderungen am Eintrag (`auth`) sind sofort in `list_entries` sichtbar.
- Die Antworten enthielten keine `Authorization`- oder `Set-Cookie`-Header. Das Geheimnis tauchte nirgends auf.
- Die Bestätigung im Dialog hat jeweils innerhalb des Zeitlimits funktioniert.
