//! Verbindungen zur Browser-Erweiterung über den Native-Messaging-Host
//! (`remember-key-browser.exe`).
//!
//! Jeder Host meldet sich mit `RegisterBrowser` an und lässt die Verbindung offen. Darüber
//! schickt die App Befehle; die Antworten kommen über eigene kurze Verbindungen
//! (`BrowserResult`) und werden hier der wartenden Anfrage zugeordnet.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rk_agent::{BrowserCommand, Response};
use serde_json::Value;

/// So lange darf die Erweiterung zum Ausfüllen brauchen.
const FILL_TIMEOUT: Duration = Duration::from_secs(15);

pub const NOT_CONNECTED: &str = "Keine Browser-Erweiterung verbunden. Bitte den Nutzer, die Remember-Key-Erweiterung \
    im Browser zu installieren bzw. den Browser zu öffnen (Einstellungen → KI-Zugriff in Remember Key).";

#[cfg(windows)]
type Pipe = rk_agent::pipe::Pipe;

#[cfg(windows)]
struct Conn {
    browser: String,
    /// Kennt `password_only` (siehe `Request::RegisterBrowser`).
    password_only: bool,
    /// Eigener Mutex, damit ein langsamer Host nicht alle anderen blockiert.
    pipe: Arc<Mutex<Pipe>>,
}

struct State {
    next_id: u64,
    #[cfg(windows)]
    conns: Vec<Conn>,
    waiting: Vec<(u64, Sender<Response>)>,
}

static STATE: Mutex<State> = Mutex::new(State {
    next_id: 1,
    #[cfg(windows)]
    conns: Vec::new(),
    waiting: Vec::new(),
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(windows)]
pub fn register(browser: String, password_only: bool, pipe: Pipe) {
    state().conns.push(Conn { browser, password_only, pipe: Arc::new(Mutex::new(pipe)) });
}

/// Ob eine verbundene Erweiterung `password_only` kennt.
pub fn supports_password_only() -> bool {
    #[cfg(windows)]
    {
        connected();
        state().conns.iter().any(|c| c.password_only)
    }
    #[cfg(not(windows))]
    false
}

/// Namen der verbundenen Browser; getrennte Verbindungen werden dabei entfernt.
pub fn connected() -> Vec<String> {
    #[cfg(windows)]
    {
        let mut s = state();
        s.conns.retain(|c| c.pipe.lock().map(|p| p.is_alive()).unwrap_or(false));
        s.conns.iter().map(|c| c.browser.clone()).collect()
    }
    #[cfg(not(windows))]
    Vec::new()
}

/// Ordnet ein Ergebnis der Erweiterung der wartenden Anfrage zu.
pub fn deliver(id: u64, result: Response) -> bool {
    let s = state();
    match s.waiting.iter().find(|(w, _)| *w == id) {
        Some((_, tx)) => tx.send(result).is_ok(),
        None => false,
    }
}

/// Schickt den Befehl an alle verbundenen Browser. Das erste erfolgreiche Ergebnis
/// gewinnt (nur ein Browser hat einen passenden Tab); sonst werden die Fehler gesammelt.
pub fn fill(
    hosts: &[String],
    host_hint: Option<&str>,
    username: &str,
    password: &str,
    otp: Option<&str>,
    password_only: bool,
) -> Result<Value, String> {
    let (tx, rx) = mpsc::channel();
    let id = {
        let mut s = state();
        let id = s.next_id;
        s.next_id += 1;
        s.waiting.push((id, tx));
        id
    };
    let result = send_and_wait(id, hosts, host_hint, username, password, otp, password_only, &rx);
    state().waiting.retain(|(w, _)| *w != id);
    result
}

#[cfg(windows)]
fn send_and_wait(
    id: u64,
    hosts: &[String],
    host_hint: Option<&str>,
    username: &str,
    password: &str,
    otp: Option<&str>,
    password_only: bool,
    rx: &mpsc::Receiver<Response>,
) -> Result<Value, String> {
    let cmd = BrowserCommand::Fill {
        id,
        hosts: hosts.to_vec(),
        host_hint: host_hint.map(str::to_string),
        username: username.to_string(),
        password: password.to_string(),
        otp: otp.map(str::to_string),
        password_only,
    };
    // `password_only` nur an Erweiterungen, die es auch beachten.
    let targets: Vec<Arc<Mutex<Pipe>>> =
        state().conns.iter().filter(|c| !password_only || c.password_only).map(|c| c.pipe.clone()).collect();
    let mut sent = 0;
    for pipe in &targets {
        let ok = pipe.lock().is_ok_and(|mut p| rk_agent::write_message(&mut *p, &cmd).is_ok());
        if ok {
            sent += 1;
        }
    }
    drop(cmd);
    // Getrennte Verbindungen aufräumen.
    connected();
    if sent == 0 {
        return Err(NOT_CONNECTED.into());
    }

    let deadline = Instant::now() + FILL_TIMEOUT;
    let mut errors: Vec<String> = Vec::new();
    for _ in 0..sent {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(Ok(v)) => return Ok(v),
            Ok(Err(e)) => errors.push(e),
            Err(_) => break,
        }
    }
    if errors.is_empty() {
        Err("Die Browser-Erweiterung hat nicht rechtzeitig geantwortet".into())
    } else {
        errors.dedup();
        Err(errors.join("; "))
    }
}

#[cfg(not(windows))]
fn send_and_wait(
    _id: u64,
    _hosts: &[String],
    _host_hint: Option<&str>,
    _username: &str,
    _password: &str,
    _otp: Option<&str>,
    _password_only: bool,
    _rx: &mpsc::Receiver<Response>,
) -> Result<Value, String> {
    Err(NOT_CONNECTED.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_without_browser_fails_fast() {
        let started = Instant::now();
        let r = fill(&["x.de".into()], None, "u", "p", None, false);
        assert_eq!(r.unwrap_err(), NOT_CONNECTED);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(state().waiting.is_empty());
    }

    #[test]
    fn deliver_unknown_id_is_false() {
        assert!(!deliver(u64::MAX, Ok(Value::Null)));
    }
}
