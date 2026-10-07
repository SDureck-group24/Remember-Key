//! MCP-Server (stdio) für Remember Key.
//!
//! Reicht Tool-Aufrufe über die Named Pipe an die laufende App weiter. Die Brücke hält
//! weder Schlüssel noch Klartexte; alle Prüfungen passieren in der App.

use std::io::{self, BufRead, Write};

use rk_agent::Request;
use serde_json::{json, Value};

const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Remember Key ist ein lokaler Passwort-Manager. Über diesen Server siehst du nur \
Einträge, die der Nutzer für KI-Assistenten freigegeben hat, und nur deren Metadaten. Passwörter, Notizen und \
2FA-Schlüssel werden nie herausgegeben – bitte den Nutzer nicht danach fragen.";

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
        _ => return tool_result(Err(format!("Unbekanntes Tool: {name}"))),
    };
    tool_result(send(&request))
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

#[cfg(windows)]
fn send(request: &Request) -> rk_agent::Response {
    use rk_agent::pipe::Pipe;

    let mut pipe = Pipe::connect().map_err(|_| "Remember Key läuft nicht. Bitte die App starten.".to_string())?;
    // Nur mit der App aus dem eigenen Installationsverzeichnis sprechen (Schutz vor
    // einem fremden Prozess, der den Pipe-Namen belegt).
    let ours = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let theirs = pipe.peer_image().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
    match (ours, theirs) {
        (Some(a), Some(b)) if rk_agent::same_path(&a, &b) => {}
        _ => return Err("Die Gegenstelle ist nicht Remember Key. Verbindung abgebrochen.".into()),
    }
    rk_agent::write_message(&mut pipe, request).map_err(|e| format!("Senden fehlgeschlagen: {e}"))?;
    rk_agent::read_message(&mut pipe).map_err(|e| format!("Keine Antwort von Remember Key: {e}"))?
}

#[cfg(not(windows))]
fn send(_request: &Request) -> rk_agent::Response {
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
    fn tools_list_has_only_metadata_tool() {
        let r = handle(&json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/list" })).unwrap();
        let names: Vec<&str> = r["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["list_entries"]);
    }
}
