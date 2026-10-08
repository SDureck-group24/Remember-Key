//! KI-Zugriff über die MCP-Brücke (`remember-key-mcp.exe`).
//!
//! Die Brücke spricht über eine Named Pipe mit der App (Protokoll in `rk_agent`). Hier
//! wird geprüft, ob der Client wirklich die Brücke aus dem App-Verzeichnis ist, ob der
//! KI-Zugriff eingeschaltet und der Tresor entsperrt ist. Jede Anfrage wird protokolliert.
//!
//! Antworten enthalten nur Metadaten freigegebener Einträge – nie Passwörter, Notizen
//! oder TOTP-Daten. Bei `http_request` setzt die App das Passwort selbst ein, nachdem der
//! Nutzer zugestimmt hat (`approval`), und bereinigt die Antwort (`http`).
//! Agent-Anfragen zählen nicht als Aktivität (Auto-Sperre bleibt wirksam).

mod approval;
mod http;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rk_agent::{AgentEntry, HttpRequest, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::vault::{UnlockedVault, VaultData};
use crate::{AppState, Inner};

pub use approval::{decide, pending, reset as reset_approvals, Decision, Pending};

const ACTIVITY_EVENT: &str = "agent-activity";
const LOG_FILE: &str = "agent-log.jsonl";
const LOG_MAX_LINES: usize = 1000;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static LOG_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Ok,
    Locked,
    Disabled,
    /// Anfrage verstößt gegen die Freigabe oder kommt von einem fremden Client.
    Rejected,
    /// Vom Nutzer abgelehnt oder nicht rechtzeitig bestätigt.
    Denied,
    /// Netzwerk- oder Serverfehler beim Ausführen.
    Failed,
}

/// Protokollzeile. Enthält nur Metadaten der Anfrage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub ts: i64,
    pub tool: String,
    pub detail: String,
    pub outcome: Outcome,
}

/// Pfad der Brücke neben der App (sofern vorhanden).
pub fn bridge_path() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?.with_file_name(rk_agent::BRIDGE_EXE);
    p.exists().then_some(p)
}

/// Die jüngsten Protokollzeilen, neueste zuerst.
pub fn read_log(limit: usize) -> Vec<LogEntry> {
    let Some(path) = LOG_PATH.get() else { return Vec::new() };
    let _g = LOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines().rev().filter_map(|l| serde_json::from_str(l).ok()).take(limit).collect()
}

fn append_log(entry: &LogEntry) {
    let Some(path) = LOG_PATH.get() else { return };
    let _g = LOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(line) = serde_json::to_string(entry) else { return };
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<&str> = text.lines().collect();
    lines.push(&line);
    let keep = &lines[lines.len().saturating_sub(LOG_MAX_LINES)..];
    if let Ok(mut f) = std::fs::File::create(path) {
        for l in keep {
            let _ = writeln!(f, "{l}");
        }
    }
}

fn log(app: &AppHandle, tool: &str, detail: String, outcome: Outcome) {
    append_log(&LogEntry { ts: crate::now(), tool: tool.into(), detail, outcome });
    let _ = app.emit(ACTIVITY_EVENT, ());
}

/// Bearbeitet eine Anfrage der Brücke.
fn handle(app: &AppHandle, req: Request) -> Response {
    match req {
        Request::ListEntries { query } => {
            let detail = query.as_deref().map(|q| format!("Suche „{q}“")).unwrap_or_default();
            let (resp, outcome) = list_entries(app, query.as_deref());
            let detail = match &resp {
                Ok(v) => {
                    let n = v["entries"].as_array().map_or(0, |a| a.len());
                    let count = if n == 1 { "1 Eintrag".to_string() } else { format!("{n} Einträge") };
                    if detail.is_empty() { count } else { format!("{detail} · {count}") }
                }
                Err(_) => detail,
            };
            log(app, "list_entries", detail, outcome);
            resp
        }
        Request::HttpRequest(req) => http_request(app, &req),
    }
}

type Refusal = (String, Outcome);

/// Entsperrter Tresor mit eingeschaltetem KI-Zugriff.
fn available(g: &Inner) -> Result<&UnlockedVault, Refusal> {
    let v = g
        .vault
        .as_ref()
        .ok_or_else(|| ("Remember Key ist gesperrt. Bitte den Nutzer, die App zu entsperren.".to_string(), Outcome::Locked))?;
    if !v.data.settings.agent_enabled {
        return Err((
            "Der KI-Zugriff ist in Remember Key ausgeschaltet (Einstellungen → KI-Zugriff).".into(),
            Outcome::Disabled,
        ));
    }
    Ok(v)
}

fn list_entries(app: &AppHandle, query: Option<&str>) -> (Response, Outcome) {
    let state = app.state::<AppState>();
    let g = state.lock();
    match available(&g) {
        Ok(v) => (Ok(json!({ "entries": visible_entries(&v.data, query) })), Outcome::Ok),
        Err((msg, outcome)) => (Err(msg), outcome),
    }
}

/// Prüft die Anfrage gegen die Freigabe des Eintrags, ohne das Geheimnis anzufassen.
fn check_request(app: &AppHandle, req: &HttpRequest) -> Result<(String, http::Prepared, u32), Refusal> {
    let state = app.state::<AppState>();
    let g = state.lock();
    let v = available(&g)?;
    let not_found = || ("Eintrag nicht gefunden oder nicht für KI-Assistenten freigegeben".to_string(), Outcome::Rejected);
    let e = v.data.entries.iter().find(|e| e.id == req.entry_id).ok_or_else(not_found)?;
    let policy = e.agent.as_ref().filter(|a| a.enabled).ok_or_else(not_found)?;
    let prepared = http::prepare(req, policy).map_err(|m| (m, Outcome::Rejected))?;
    Ok((e.title.clone(), prepared, policy.session_minutes))
}

/// Holt Benutzername und Passwort nach der Zustimmung. Die Freigabe wird erneut geprüft,
/// weil sie sich während des Dialogs geändert haben kann.
fn credentials(app: &AppHandle, req: &HttpRequest, approved: &http::Prepared) -> Result<http::Credentials, Refusal> {
    let state = app.state::<AppState>();
    let g = state.lock();
    let v = available(&g)?;
    let changed = || ("Die Freigabe wurde während der Bestätigung geändert".to_string(), Outcome::Rejected);
    let e = v.data.entries.iter().find(|e| e.id == req.entry_id).ok_or_else(changed)?;
    let policy = e.agent.as_ref().filter(|a| a.enabled).ok_or_else(changed)?;
    let again = http::prepare(req, policy).map_err(|_| changed())?;
    if again.auth != approved.auth || again.host != approved.host {
        return Err(changed());
    }
    http::Credentials::from_entry(e)
        .ok_or_else(|| ("Für diesen Eintrag ist weder API-Token noch Passwort hinterlegt".into(), Outcome::Rejected))
}

/// Host und Pfad einer URL fürs Protokoll (ohne Query, die Daten enthalten kann).
fn short_url(url: &str) -> String {
    url::Url::parse(url.trim())
        .ok()
        .and_then(|u| Some(format!("{}{}", u.host_str()?, u.path())))
        .unwrap_or_else(|| "ungültige URL".into())
}

fn http_request(app: &AppHandle, req: &HttpRequest) -> Response {
    const TOOL: &str = "http_request";
    let refuse = |(msg, outcome): Refusal, detail: String| -> Response {
        log(app, TOOL, format!("{detail} · {msg}"), outcome);
        Err(msg)
    };
    let request_line = format!("{} {}", req.method.trim().to_ascii_uppercase(), short_url(&req.url));

    let (title, prepared, session_minutes) = match check_request(app, req) {
        Ok(x) => x,
        Err(r) => return refuse(r, request_line),
    };
    let detail = format!("{} · {title}", prepared.summary());
    let key = (req.entry_id.clone(), prepared.host.clone(), prepared.auth.key());

    let approval_note = if approval::has_grant(&key) {
        "Sitzungsfreigabe"
    } else {
        let pending = Pending {
            id: 0,
            entry_title: title.clone(),
            method: prepared.method.clone(),
            host: prepared.host.clone(),
            path: prepared.url.path().to_string(),
            auth: prepared.auth.key(),
            session_minutes,
            expires_at: 0,
        };
        match approval::ask(app, pending, key) {
            Decision::Once => "einmal bestätigt",
            Decision::Session => "für Sitzung bestätigt",
            Decision::Deny => {
                return refuse(("Der Nutzer hat die Anfrage in Remember Key abgelehnt.".into(), Outcome::Denied), detail)
            }
            Decision::Timeout => {
                return refuse(
                    ("Keine Bestätigung in Remember Key (Zeit abgelaufen oder Tresor gesperrt).".into(), Outcome::Denied),
                    detail,
                )
            }
        }
    };

    let creds = match credentials(app, req, &prepared) {
        Ok(c) => c,
        Err(r) => return refuse(r, detail),
    };
    let source = creds.source.key();
    let result = http::execute(&prepared, &creds);
    drop(creds);
    match result {
        Ok(v) => {
            log(app, TOOL, format!("{detail} · {} · {source} · {approval_note}", v["status"]), Outcome::Ok);
            Ok(v)
        }
        Err(msg) => {
            log(app, TOOL, format!("{detail} · {msg}"), Outcome::Failed);
            Err(msg)
        }
    }
}

/// Freigegebene Einträge (nur Metadaten), optional gefiltert, nach Titel sortiert.
fn visible_entries(data: &VaultData, query: Option<&str>) -> Vec<AgentEntry> {
    let q = query.map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
    let mut entries: Vec<AgentEntry> = data
        .entries
        .iter()
        .filter_map(|e| {
            let policy = e.agent.as_ref().filter(|a| a.enabled)?;
            let matches = q.as_deref().is_none_or(|q| {
                e.title.to_lowercase().contains(q)
                    || e.username.to_lowercase().contains(q)
                    || policy.hosts.iter().any(|h| h.contains(q))
            });
            matches.then(|| AgentEntry {
                id: e.id.clone(),
                title: e.title.clone(),
                username: e.username.clone(),
                hosts: policy.hosts.clone(),
                auth: policy.auth.iter().map(|a| a.key()).collect(),
                secret: http::SecretSource::for_entry(e).map(|s| s.key().to_string()),
                has_totp: e.totp.is_some(),
            })
        })
        .collect();
    entries.sort_by_key(|e| e.title.to_lowercase());
    entries
}

#[cfg(windows)]
pub fn spawn(app: AppHandle, dir: &Path) {
    use rk_agent::pipe::Pipe;
    use std::thread;
    use std::time::Duration;

    let _ = LOG_PATH.set(dir.join(LOG_FILE));
    let Ok(exe) = std::env::current_exe() else { return };
    let expected = exe.with_file_name(rk_agent::BRIDGE_EXE);

    thread::spawn(move || {
        let mut first = true;
        loop {
            let pipe = match Pipe::create_server(first) {
                Ok(p) => p,
                // Erste Instanz scheitert, wenn der Name schon belegt ist (zweite App-Instanz
                // oder fremder Prozess) – dann bleibt der KI-Zugriff aus.
                Err(e) if first => {
                    eprintln!("KI-Zugriff nicht verfügbar: {e}");
                    return;
                }
                Err(_) => {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
            };
            first = false;
            if pipe.accept().is_err() {
                continue;
            }
            let app = app.clone();
            let expected = expected.clone();
            thread::spawn(move || serve(&app, pipe, &expected));
        }
    });
}

#[cfg(windows)]
fn serve(app: &AppHandle, mut pipe: rk_agent::pipe::Pipe, expected: &Path) {
    let client = pipe.peer_image();
    if !client.as_ref().is_ok_and(|p| rk_agent::same_path(p, expected)) {
        let who = client.map(|p| p.display().to_string()).unwrap_or_else(|_| "unbekannt".into());
        log(app, "verbindung", format!("Nicht zugelassener Client: {who}"), Outcome::Rejected);
        // Erst die Anfrage abnehmen: Beide Seiten flushen nach dem Schreiben, und
        // FlushFileBuffers wartet, bis die Gegenseite gelesen hat. Ohne dieses Lesen würden
        // Client und App dauerhaft aufeinander warten.
        let _ = rk_agent::read_message::<serde_json::Value>(&mut pipe);
        let _ = rk_agent::write_message(&mut pipe, &Response::Err("Nicht zugelassener Client".into()));
        return;
    }
    let Ok(req) = rk_agent::read_message::<Request>(&mut pipe) else { return };
    let resp = handle(app, req);
    let _ = rk_agent::write_message(&mut pipe, &resp);
}

#[cfg(not(windows))]
pub fn spawn(_app: AppHandle, dir: &Path) {
    let _ = LOG_PATH.set(dir.join(LOG_FILE));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::{AgentPolicy, Entry};

    fn entry(id: &str, title: &str, agent: Option<AgentPolicy>) -> Entry {
        Entry {
            id: id.into(),
            title: title.into(),
            username: "me@example.com".into(),
            password: "geheim-123".into(),
            api_token: "tok-geheim".into(),
            url: String::new(),
            notes: "private Notiz".into(),
            totp: None,
            folder_id: None,
            created_at: 0,
            updated_at: 0,
            agent,
        }
    }

    fn allowed(host: &str) -> Option<AgentPolicy> {
        Some(AgentPolicy { enabled: true, hosts: vec![host.into()], ..Default::default() })
    }

    #[test]
    fn only_enabled_entries_without_secrets() {
        let mut d = VaultData::default();
        d.entries.push(entry("1", "GitHub", allowed("github.com")));
        d.entries.push(entry("2", "Bank", None));
        d.entries.push(entry("3", "Alt", Some(AgentPolicy { enabled: false, hosts: vec!["alt.de".into()], ..Default::default() })));
        d.entries.push(entry("4", "Azure", allowed("*.azure.com")));
        let list = visible_entries(&d, None);
        let titles: Vec<&str> = list.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["Azure", "GitHub"]);
        let json = serde_json::to_string(&list).unwrap();
        assert!(!json.contains("geheim-123") && !json.contains("tok-geheim") && !json.contains("private Notiz"));
        assert_eq!(list[0].secret.as_deref(), Some("apiToken"));
    }

    #[test]
    fn query_matches_title_username_and_host() {
        let mut d = VaultData::default();
        d.entries.push(entry("1", "Code", allowed("github.com")));
        d.entries.push(entry("2", "Mail", allowed("mail.example.com")));
        assert_eq!(visible_entries(&d, Some("GITHUB")).len(), 1);
        assert_eq!(visible_entries(&d, Some("mail")).len(), 1);
        assert_eq!(visible_entries(&d, Some("me@example")).len(), 2);
        assert_eq!(visible_entries(&d, Some("  ")).len(), 2);
        assert!(visible_entries(&d, Some("bank")).is_empty());
    }
}
