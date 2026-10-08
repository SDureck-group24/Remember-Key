//! Bestätigung durch den Nutzer, bevor ein Geheimnis eingesetzt wird.
//!
//! Die Anfrage wartet (ohne gehaltenen Tresor-Mutex) bis zu 60 s auf eine Entscheidung im
//! Fenster. Sitzungsfreigaben gelten pro Eintrag, Host und Einsetz-Stelle und verfallen
//! spätestens beim Sperren des Tresors.

use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, UserAttentionType};

const APPROVAL_EVENT: &str = "agent-approval";
const APPROVAL_DONE_EVENT: &str = "agent-approval-done";
pub const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Deny,
    Once,
    Session,
    /// Keine Antwort innerhalb der Frist oder Tresor gesperrt.
    #[serde(skip_deserializing)]
    Timeout,
}

/// Was der Nutzer im Dialog sieht.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub id: u64,
    /// `http` (Anfrage mit eingesetztem Geheimnis) oder `fill` (Login im Browser).
    pub action: String,
    pub entry_title: String,
    pub method: String,
    pub host: String,
    pub path: String,
    pub auth: String,
    /// 0 = keine Sitzungsfreigabe möglich.
    pub session_minutes: u32,
    /// Unix-Zeit, zu der die Anfrage automatisch abgelehnt wird.
    pub expires_at: i64,
}

/// Schlüssel einer Sitzungsfreigabe: Eintrag, Host, Einsetz-Stelle.
pub type GrantKey = (String, String, String);

struct Queue {
    next_id: u64,
    pending: Vec<(Pending, Sender<Decision>)>,
    /// Sitzungsfreigaben mit Ablaufzeit (wenige Einträge, daher eine Liste).
    grants: Vec<(GrantKey, Instant)>,
}

static QUEUE: Mutex<Queue> = Mutex::new(Queue { next_id: 1, pending: Vec::new(), grants: Vec::new() });

fn queue() -> std::sync::MutexGuard<'static, Queue> {
    QUEUE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Besteht eine noch gültige Sitzungsfreigabe?
pub fn has_grant(key: &GrantKey) -> bool {
    let mut q = queue();
    let now = Instant::now();
    q.grants.retain(|(_, until)| *until > now);
    q.grants.iter().any(|(k, _)| k == key)
}

/// Fragt den Nutzer und wartet blockierend auf die Antwort.
pub fn ask(app: &AppHandle, mut pending: Pending, key: GrantKey) -> Decision {
    let (tx, rx) = mpsc::channel();
    let id = {
        let mut q = queue();
        let id = q.next_id;
        q.next_id += 1;
        pending.id = id;
        pending.expires_at = crate::now() + TIMEOUT.as_secs() as i64;
        q.pending.push((pending.clone(), tx));
        id
    };
    let _ = app.emit(APPROVAL_EVENT, &pending);
    bring_to_front(app);

    let decision = rx.recv_timeout(TIMEOUT).unwrap_or(Decision::Timeout);
    // Bei Zeitablauf steht die Anfrage noch in der Warteschlange.
    queue().pending.retain(|(p, _)| p.id != id);
    let _ = app.emit(APPROVAL_DONE_EVENT, id);

    if decision == Decision::Session && pending.session_minutes > 0 {
        let until = Instant::now() + Duration::from_secs(pending.session_minutes as u64 * 60);
        let mut q = queue();
        q.grants.retain(|(k, _)| *k != key);
        q.grants.push((key, until));
    }
    decision
}

/// Antwort aus dem Fenster. `false`, wenn die Anfrage nicht (mehr) wartet.
pub fn decide(id: u64, decision: Decision) -> bool {
    let mut q = queue();
    let Some(pos) = q.pending.iter().position(|(p, _)| p.id == id) else { return false };
    let (_, tx) = q.pending.remove(pos);
    tx.send(decision).is_ok()
}

/// Wartende Anfragen (z. B. nach dem Neuladen des Fensters).
pub fn pending() -> Vec<Pending> {
    queue().pending.iter().map(|(p, _)| p.clone()).collect()
}

/// Beim Sperren: wartende Anfragen ablehnen, Sitzungsfreigaben verwerfen.
pub fn reset() {
    let mut q = queue();
    for (_, tx) in q.pending.drain(..) {
        let _ = tx.send(Decision::Timeout);
    }
    q.grants.clear();
}

fn bring_to_front(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        // Falls Windows das Fokussieren verhindert: in der Taskleiste blinken.
        let _ = w.request_user_attention(Some(UserAttentionType::Critical));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Die Warteschlange ist global; die Tests nutzen eigene IDs/Schlüssel und laufen
    // trotzdem seriell, damit `reset` keinen anderen Test stört.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn key(e: &str) -> GrantKey {
        (e.into(), "x.de".into(), "bearer".into())
    }

    #[test]
    fn decide_unknown_id_is_false() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!decide(u64::MAX, Decision::Once));
    }

    #[test]
    fn grants_expire_and_reset() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        queue().grants.push((key("a"), Instant::now() + Duration::from_secs(60)));
        queue().grants.push((key("b"), Instant::now() - Duration::from_secs(1)));
        assert!(has_grant(&key("a")));
        assert!(!has_grant(&key("b")));
        assert!(!has_grant(&("a".into(), "y.de".into(), "bearer".into())));
        reset();
        assert!(!has_grant(&key("a")));
    }

    #[test]
    fn reset_releases_waiting_requests() {
        let _s = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (tx, rx) = mpsc::channel();
        let p = Pending {
            id: 4242,
            action: "http".into(),
            entry_title: String::new(),
            method: "GET".into(),
            host: "x.de".into(),
            path: "/".into(),
            auth: "bearer".into(),
            session_minutes: 0,
            expires_at: 0,
        };
        queue().pending.push((p, tx));
        reset();
        assert_eq!(rx.recv().unwrap(), Decision::Timeout);
        assert!(pending().iter().all(|p| p.id != 4242));
    }
}
