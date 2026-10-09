//! Cookie-Sitzungen für `http_request`.
//!
//! Manche APIs (z. B. die Acumatica-Customization-API) merken sich die Anmeldung über ein
//! Session-Cookie. Die App behält die Cookies einer Login-Anfrage selbst; die KI bekommt nur
//! eine zufällige Sitzungs-ID. Eine Sitzung ist an Eintrag, Host und Einsetz-Stelle des
//! Logins gebunden, verfällt nach 15 Minuten ohne Nutzung, spätestens nach einer Stunde und
//! beim Sperren des Tresors. Folgeanfragen werden trotzdem einzeln gegen die Freigabe geprüft
//! und – ohne Sitzungsfreigabe – einzeln bestätigt.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use data_encoding::HEXLOWER;
use rand::rngs::OsRng;
use rand::RngCore;

use super::http::Jar;
use crate::vault::AuthLocation;

pub const IDLE: Duration = Duration::from_secs(15 * 60);
const MAX_AGE: Duration = Duration::from_secs(60 * 60);
const MAX_SESSIONS: usize = 20;

/// Wert von `session`, der eine neue Sitzung anlegt.
pub const NEW: &str = "new";

/// Wozu eine Sitzung gehört (ohne Cookies).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub entry_id: String,
    pub host: String,
    pub auth: AuthLocation,
}

struct Session {
    binding: Binding,
    /// `None`, solange eine Anfrage die Cookies benutzt.
    jar: Option<Jar>,
    created: Instant,
    last_used: Instant,
}

impl Session {
    fn expired(&self, now: Instant) -> bool {
        now.duration_since(self.last_used) > IDLE || now.duration_since(self.created) > MAX_AGE
    }
}

static SESSIONS: Mutex<Option<HashMap<String, Session>>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut HashMap<String, Session>) -> R) -> R {
    let mut g = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    // Laufende Anfragen nicht abräumen; sie geben die Sitzung gleich zurück.
    map.retain(|_, s| s.jar.is_none() || !s.expired(now));
    f(map)
}

pub const UNKNOWN: &str =
    "Sitzung unbekannt oder abgelaufen (15 Min. ohne Nutzung, höchstens 1 Std., endet beim Sperren). Bitte neu \
     anmelden: Login-Anfrage mit session: \"new\".";

/// Bindung einer bestehenden Sitzung.
pub fn binding(id: &str) -> Result<Binding, String> {
    with(|m| m.get(id).map(|s| s.binding.clone())).ok_or_else(|| UNKNOWN.to_string())
}

/// Legt eine Sitzung mit den Cookies einer erfolgreichen Login-Anfrage an.
pub fn open(binding: Binding, jar: Jar) -> String {
    let mut raw = [0u8; 16];
    OsRng.fill_bytes(&mut raw);
    let id = format!("s_{}", HEXLOWER.encode(&raw));
    with(|m| {
        // Bei zu vielen Sitzungen die am längsten unbenutzte (nicht laufende) verwerfen.
        if m.len() >= MAX_SESSIONS {
            let oldest = m.iter().filter(|(_, s)| s.jar.is_some()).min_by_key(|(_, s)| s.last_used).map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                m.remove(&k);
            }
        }
        let now = Instant::now();
        m.insert(id.clone(), Session { binding, jar: Some(jar), created: now, last_used: now });
    });
    id
}

/// Nimmt die Cookies für eine Anfrage heraus. Die Bindung muss noch dieselbe sein.
pub fn checkout(id: &str, expected: &Binding) -> Result<Jar, String> {
    with(|m| {
        let s = m.get_mut(id).ok_or_else(|| UNKNOWN.to_string())?;
        if s.binding != *expected {
            return Err(UNKNOWN.to_string());
        }
        s.jar.take().ok_or_else(|| {
            "Diese Sitzung bearbeitet gerade eine andere Anfrage. Anfragen einer Sitzung bitte nacheinander senden.".into()
        })
    })
}

/// Gibt die (aktualisierten) Cookies zurück. Mit `end` oder ohne Cookies endet die Sitzung.
/// Wurde die Sitzung inzwischen verworfen (Tresor gesperrt), gehen die Cookies verloren.
pub fn checkin(id: &str, jar: Jar, end: bool) {
    with(|m| {
        if end || jar.is_empty() {
            m.remove(id);
        } else if let Some(s) = m.get_mut(id) {
            s.jar = Some(jar);
            s.last_used = Instant::now();
        }
    });
}

/// Beendet eine Sitzung (z. B. wenn die Freigabe entzogen wurde).
pub fn close(id: &str) {
    with(|m| m.remove(id));
}

/// Beim Sperren: alle Sitzungen verwerfen. `Jar` nullt die Cookies beim Drop.
pub fn clear() {
    let mut g = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    *g = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bind(entry: &str) -> Binding {
        Binding { entry_id: entry.into(), host: "x.de".into(), auth: AuthLocation::JsonField { name: "password".into() } }
    }

    fn jar() -> Jar {
        let mut j = Jar::default();
        j.store("a=b");
        j
    }

    #[test]
    fn lifecycle() {
        let id = open(bind("sess-1"), jar());
        assert!(id.starts_with("s_") && id.len() == 34);
        assert_eq!(binding(&id).unwrap(), bind("sess-1"));
        // Falsche Bindung (anderer Eintrag) wird abgewiesen.
        assert!(checkout(&id, &bind("sess-x")).is_err());
        let j = checkout(&id, &bind("sess-1")).unwrap();
        // Zweite gleichzeitige Anfrage derselben Sitzung
        assert!(checkout(&id, &bind("sess-1")).err().unwrap().contains("nacheinander"));
        checkin(&id, j, false);
        let j = checkout(&id, &bind("sess-1")).unwrap();
        checkin(&id, j, true);
        assert!(binding(&id).is_err());
    }

    #[test]
    fn empty_jar_ends_session() {
        let id = open(bind("sess-2"), jar());
        let _ = checkout(&id, &bind("sess-2")).unwrap();
        checkin(&id, Jar::default(), false);
        assert!(binding(&id).is_err());
    }

    #[test]
    fn expiry() {
        let now = Instant::now();
        let s = Session { binding: bind("e"), jar: Some(jar()), created: now, last_used: now };
        assert!(!s.expired(now));
        assert!(s.expired(now + IDLE + Duration::from_secs(1)));
        let s = Session { binding: bind("e"), jar: Some(jar()), created: now, last_used: now + MAX_AGE };
        assert!(s.expired(now + MAX_AGE + Duration::from_secs(1)));
    }
}
