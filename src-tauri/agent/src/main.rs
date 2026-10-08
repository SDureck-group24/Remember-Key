//! MCP-Server (stdio) für Remember Key.
//!
//! Reicht Tool-Aufrufe über die Named Pipe an die laufende App weiter. Die Brücke hält
//! weder Schlüssel noch Klartexte; alle Prüfungen passieren in der App.

use std::io::{self, BufRead, Write};

use std::collections::BTreeMap;
use std::time::Duration;

use rk_agent::{FillLogin, HttpRequest, Request};
use serde_json::{json, Value};

const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Remember Key ist ein lokaler Passwort-Manager. Über diesen Server siehst du nur \
Einträge, die der Nutzer für KI-Assistenten freigegeben hat, und nur deren Metadaten. Passwörter, Notizen und \
2FA-Schlüssel werden nie herausgegeben – bitte den Nutzer nicht danach fragen. Mit http_request kannst du \
authentifizierte HTTPS-Anfragen stellen: Remember Key setzt die Zugangsdaten selbst ein, nachdem der Nutzer \
zugestimmt hat.";

fn main() {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&msg),
            Err(_) => Some(error(Value::Null, -32700, "Ungültiges JSON")),
        };
        if let Some(reply) = reply {
            if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
}

/// Beantwortet eine JSON-RPC-Anfrage. Benachrichtigungen und Antworten des Clients
/// (ohne `method` oder ohne `id`) bleiben unbeantwortet.
fn handle(msg: &Value) -> Option<Value> {
    let method = msg.get("method")?.as_str().unwrap_or_default();
    let id = msg.get("id")?.clone();
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    let result = match method {
        "initialize" => initialize(&params),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => call_tool(&params),
        _ => return Some(error(id, -32601, &format!("Unbekannte Methode: {method}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
    let version = PROTOCOL_VERSIONS.iter().find(|v| **v == requested).unwrap_or(&PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "remember-key", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

fn tools() -> Value {
    json!([
        {
            "name": "list_entries",
            "title": "Freigegebene Einträge auflisten",
            "description": "Listet die Einträge aus Remember Key, die für KI-Assistenten freigegeben sind: ID, Titel, \
                Benutzername, gebundene Hosts und ob ein 2FA-Schlüssel hinterlegt ist. Passwörter, Notizen und \
                2FA-Schlüssel werden nie zurückgegeben.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Optionaler Suchbegriff; filtert nach Titel, Benutzername oder Host."
                    }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false }
        },
        {
            "name": "http_request",
            "title": "Authentifizierte HTTPS-Anfrage",
            "description": "Führt eine HTTPS-Anfrage aus, in die Remember Key die Zugangsdaten eines freigegebenen \
                Eintrags selbst einsetzt (z. B. als Bearer-Token oder Basic-Auth). Du siehst das Passwort nie; es \
                wird auch aus der Antwort entfernt. Nur an die Hosts des Eintrags (siehe list_entries). Der Nutzer \
                muss die Anfrage in Remember Key bestätigen – das kann bis zu 60 Sekunden dauern. Weiterleitungen \
                werden nicht verfolgt. Setze selbst keine Authorization- oder Cookie-Header. Die Antwort nennt unter \
                `injected`, wo die Zugangsdaten eingesetzt wurden. Bei wiederholtem 401/403 nicht einfach erneut \
                versuchen (jeder Versuch kostet eine Bestätigung), sondern den Nutzer bitten, Token und Freigabe zu prüfen.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entry_id": { "type": "string", "description": "ID des Eintrags aus list_entries." },
                    "url": {
                        "type": "string",
                        "description": "Vollständige https-URL (http nur für localhost/127.0.0.1); der Host muss zum Eintrag passen."
                    },
                    "method": {
                        "type": "string",
                        "enum": ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"],
                        "default": "GET"
                    },
                    "headers": {
                        "type": "object",
                        "additionalProperties": { "type": "string" },
                        "description": "Zusätzliche Header, z. B. Accept oder Content-Type."
                    },
                    "body": { "type": "string", "description": "Request-Body (nicht bei GET/HEAD)." },
                    "auth": {
                        "type": "string",
                        "description": "Einsetz-Stelle aus list_entries (bearer, basic, header:<Name>). Nur nötig, \
                            wenn mehrere erlaubt sind."
                    }
                },
                "required": ["entry_id", "url"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "openWorldHint": true }
        },
        {
            "name": "fill_login",
            "title": "Login im Browser ausfüllen",
            "description": "Lässt Remember Key über seine Browser-Erweiterung das Login-Formular im offenen Tab                 ausfüllen und absenden – Benutzername und Passwort eines freigegebenen Eintrags (fillLogin in                 list_entries). Du siehst das Passwort nie und gibst es auch nicht selbst ein. Öffne vorher die                 Login-Seite; sie muss zu den Hosts des Eintrags passen. Der Nutzer bestätigt in Remember Key (bis zu                 60 Sekunden). Bei zweistufigen Logins (erst Benutzername, dann Passwort) einfach erneut aufrufen,                 sobald das Passwortfeld angezeigt wird. Lies das Passwortfeld danach nicht aus.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entry_id": { "type": "string", "description": "ID des Eintrags aus list_entries." },
                    "url": {
                        "type": "string",
                        "description": "Optional: Adresse der geöffneten Login-Seite, falls mehrere passende Tabs offen sind."
                    }
                },
                "required": ["entry_id"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true }
        }
    ])
}

fn call_tool(params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let request = match name {
        "list_entries" => Request::ListEntries {
            query: args.get("query").and_then(Value::as_str).map(str::to_string).filter(|q| !q.trim().is_empty()),
        },
        "http_request" => match http_request(&args) {
            Ok(r) => Request::HttpRequest(r),
            Err(e) => return tool_result(Err(e)),
        },
        "fill_login" => match args.get("entry_id").and_then(Value::as_str) {
            Some(id) => Request::FillLogin(FillLogin {
                entry_id: id.to_string(),
                url: args.get("url").and_then(Value::as_str).map(str::to_string).filter(|u| !u.trim().is_empty()),
            }),
            None => return tool_result(Err("entry_id fehlt".into())),
        },
        _ => return tool_result(Err(format!("Unbekanntes Tool: {name}"))),
    };
    tool_result(send(request))
}

/// Höchstwartezeit auf die App. Bei `http_request` umfasst sie den Bestätigungsdialog
/// (60 s) sowie Verbindungsaufbau (10 s) und Anfrage (30 s) in der App, plus Reserve.
fn timeout_for(request: &Request) -> Duration {
    match request {
        Request::ListEntries { .. } => Duration::from_secs(15),
        Request::HttpRequest(_) => Duration::from_secs(120),
        // Bestätigung (60 s) und Ausfüllen in der Erweiterung (15 s), plus Reserve.
        Request::FillLogin(_) => Duration::from_secs(100),
        // Werden von der Brücke nie gesendet.
        Request::RegisterBrowser { .. } | Request::BrowserResult { .. } => Duration::from_secs(15),
    }
}

fn http_request(args: &Value) -> Result<HttpRequest, String> {
    let text = |key: &str| args.get(key).and_then(Value::as_str).map(str::to_string);
    let mut headers = BTreeMap::new();
    if let Some(h) = args.get("headers").filter(|h| !h.is_null()) {
        let obj = h.as_object().ok_or("headers muss ein Objekt sein")?;
        for (k, v) in obj {
            let v = v.as_str().ok_or_else(|| format!("Header {k}: Wert muss ein Text sein"))?;
            headers.insert(k.clone(), v.to_string());
        }
    }
    Ok(HttpRequest {
        entry_id: text("entry_id").ok_or("entry_id fehlt")?,
        method: text("method").unwrap_or_else(|| "GET".into()),
        url: text("url").ok_or("url fehlt")?,
        headers,
        body: text("body"),
        auth: text("auth"),
    })
}

fn tool_result(result: rk_agent::Response) -> Value {
    match result {
        Ok(value) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
            "isError": false,
        }),
        Err(message) => json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
    }
}

/// Schickt die Anfrage an die App. Hängt die App (oder beendet sie sich gerade bei noch
/// offener Pipe), bricht die Brücke nach `timeout_for` ab, statt den Tool-Aufruf ewig zu
/// blockieren.
#[cfg(windows)]
fn send(request: Request) -> rk_agent::Response {
    use std::os::windows::io::AsRawHandle;
    use std::sync::mpsc;
    use std::thread;

    let timeout = timeout_for(&request);
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let _ = tx.send(exchange(&request));
    });
    match rx.recv_timeout(timeout) {
        Ok(resp) => resp,
        Err(_) => {
            // Blockierendes ReadFile/WriteFile des Workers abbrechen; dadurch schließt sich
            // die Verbindung und auch die App bekommt einen Fehler statt weiter zu warten.
            unsafe { windows_sys::Win32::System::IO::CancelSynchronousIo(worker.as_raw_handle()) };
            Err(format!(
                "Remember Key hat nicht innerhalb von {} s geantwortet. Bitte den Nutzer prüfen lassen, ob die App \
                 läuft und nicht hängt.",
                timeout.as_secs()
            ))
        }
    }
}

#[cfg(windows)]
fn exchange(request: &Request) -> rk_agent::Response {
    let mut pipe = rk_agent::pipe::connect_to_app()?;
    rk_agent::write_message(&mut pipe, request).map_err(|e| format!("Senden fehlgeschlagen: {e}"))?;
    rk_agent::read_message(&mut pipe).map_err(|e| format!("Keine Antwort von Remember Key: {e}"))?
}

#[cfg(not(windows))]
fn send(_request: Request) -> rk_agent::Response {
    Err("Remember Key ist nur unter Windows verfügbar.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_get_no_reply() {
        assert!(handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
    }

    #[test]
    fn initialize_echoes_supported_version() {
        let r = handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2025-03-26" } }))
        .unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        let r = handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize",
            "params": { "protocolVersion": "1999-01-01" } }))
        .unwrap();
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    }

    #[test]
    fn unknown_method_and_tool() {
        let r = handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/list" })).unwrap();
        assert_eq!(r["error"]["code"], -32601);
        let r = handle(&json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": { "name": "reveal_password" } }))
        .unwrap();
        assert_eq!(r["result"]["isError"], true);
    }

    #[test]
    fn tools_list_names() {
        let r = handle(&json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/list" })).unwrap();
        let names: Vec<&str> = r["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["list_entries", "http_request", "fill_login"]);
    }

    #[test]
    fn http_request_arguments() {
        let r = http_request(&json!({ "entry_id": "1", "url": "https://x.de", "headers": { "Accept": "a" } })).unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.headers["Accept"], "a");
        assert!(http_request(&json!({ "url": "https://x.de" })).is_err());
        assert!(http_request(&json!({ "entry_id": "1", "url": "u", "headers": { "A": 1 } })).is_err());
        assert!(http_request(&json!({ "entry_id": "1", "url": "u", "headers": "A: b" })).is_err());
    }

    #[test]
    fn timeouts_cover_approval_and_request() {
        assert!(timeout_for(&Request::ListEntries { query: None }) <= Duration::from_secs(15));
        let req = http_request(&json!({ "entry_id": "1", "url": "https://x.de" })).unwrap();
        // Dialog (60 s) + Verbindungsaufbau (10 s) + Anfrage (30 s) in der App
        assert!(timeout_for(&Request::HttpRequest(req)) > Duration::from_secs(100));
    }
}
