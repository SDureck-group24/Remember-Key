//! Native-Messaging-Host für die Remember-Key-Browser-Erweiterung.
//!
//! Der Browser startet dieses Programm, sobald die Erweiterung `connectNative` aufruft, und
//! spricht über stdin/stdout im Native-Messaging-Format (u32-Länge + JSON). Das Programm
//! verbindet die Erweiterung mit der App:
//!
//! - Verbindung A zur App bleibt offen. Darüber schickt die App `BrowserCommand`s, die
//!   unverändert an die Erweiterung gehen.
//! - Jede Antwort der Erweiterung geht über eine neue, kurze Verbindung zurück.
//!
//! Zwei Richtungen über getrennte Verbindungen, weil synchrone Pipe-Handles Lese- und
//! Schreibzugriffe serialisieren: ein wartendes ReadFile würde jedes WriteFile blockieren.

use std::io::{self, Read, Write};

use serde_json::{json, Value};

/// Obergrenze für Nachrichten der Erweiterung.
const MAX_NATIVE_MESSAGE: usize = 1024 * 1024;

fn read_native(r: &mut impl Read) -> io::Result<Value> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_ne_bytes(len) as usize;
    if len > MAX_NATIVE_MESSAGE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Nachricht zu groß"));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}

fn write_native(w: &mut impl Write, value: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    w.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    w.write_all(&bytes)?;
    w.flush()
}

#[cfg(windows)]
fn main() {
    use rk_agent::{read_message, write_message, Request, Response};
    use std::process::exit;
    use std::thread;

    let mut input = io::stdin().lock();
    // Erste Nachricht der Erweiterung: {"type":"hello","browser":"…"}
    let Ok(hello) = read_native(&mut input) else { return };
    let browser: String = hello["browser"].as_str().unwrap_or("Browser").chars().take(40).collect();

    let mut pipe = match rk_agent::pipe::connect_to_app() {
        Ok(p) => p,
        Err(message) => {
            let _ = write_native(&mut io::stdout().lock(), &json!({ "type": "error", "message": message }));
            return;
        }
    };
    if write_message(&mut pipe, &Request::RegisterBrowser { browser }).is_err() {
        return;
    }
    let _ = write_native(&mut io::stdout().lock(), &json!({ "type": "connected" }));

    // App → Erweiterung. Endet die Verbindung (App beendet), endet auch der Host; die
    // Erweiterung verbindet sich später neu.
    thread::spawn(move || loop {
        match read_message::<Value>(&mut pipe) {
            Ok(cmd) => {
                if write_native(&mut io::stdout().lock(), &cmd).is_err() {
                    exit(0);
                }
            }
            Err(_) => exit(0),
        }
    });

    // Erweiterung → App
    loop {
        let Ok(msg) = read_native(&mut input) else { exit(0) };
        if msg["type"] != "result" {
            continue;
        }
        let Some(id) = msg["id"].as_u64() else { continue };
        let result: Response = serde_json::from_value(msg["result"].clone())
            .unwrap_or_else(|_| Err("Ungültige Antwort der Browser-Erweiterung".into()));
        if let Ok(mut conn) = rk_agent::pipe::connect_to_app() {
            let _ = write_message(&mut conn, &Request::BrowserResult { id, result });
            // Auf die Bestätigung der App warten, damit sie die Nachricht vollständig liest.
            let _ = read_message::<Response>(&mut conn);
        }
    }
}

#[cfg(not(windows))]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_framing_roundtrip() {
        let mut buf = Vec::new();
        write_native(&mut buf, &json!({ "type": "hello", "browser": "Chrome" })).unwrap();
        assert_eq!(u32::from_ne_bytes(buf[..4].try_into().unwrap()) as usize, buf.len() - 4);
        let v = read_native(&mut buf.as_slice()).unwrap();
        assert_eq!(v["browser"], "Chrome");
    }

    #[test]
    fn rejects_oversized_native_message() {
        let mut buf = ((MAX_NATIVE_MESSAGE + 1) as u32).to_ne_bytes().to_vec();
        buf.extend_from_slice(b"{}");
        assert!(read_native(&mut buf.as_slice()).is_err());
    }
}
