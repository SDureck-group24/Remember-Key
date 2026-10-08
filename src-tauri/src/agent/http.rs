//! HTTPS-Anfragen im Auftrag der KI, bei denen die App das Passwort selbst einsetzt.
//!
//! Regeln:
//! - nur `https` (unverschlüsseltes `http` nur für Loopback-Adressen, die den Rechner nicht
//!   verlassen), nur an die Hosts der Freigabe, keine Zugangsdaten in der URL
//! - das Geheimnis landet ausschließlich an der gewählten, freigegebenen Stelle (Header);
//!   Body und URL stammen allein von der KI, die das Geheimnis nicht kennt
//! - die KI darf keine Authorization-, Cookie- oder Verbindungs-Header setzen
//! - Redirects werden nicht verfolgt (jeder Folgeaufruf wird neu geprüft)
//! - aus der Antwort werden Geheimnis und gängige Kodierungen entfernt, Cookies und
//!   Auth-Header fallen ganz weg

use std::io::Read;
use std::time::Duration;

use data_encoding::{BASE64, BASE64URL, BASE64URL_NOPAD, BASE64_NOPAD, HEXLOWER, HEXUPPER};
use rk_agent::HttpRequest;
use serde_json::{json, Map, Value};
use url::Url;
use zeroize::Zeroizing;

use crate::vault::{AgentPolicy, AuthLocation, Entry, FORBIDDEN_HEADERS};

/// Hosts, für die auch `http` erlaubt ist (Verkehr bleibt auf dem Rechner).
const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];
const METHODS: [&str; 6] = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"];
const MAX_HEADERS: usize = 50;
/// Größe des Antwort-Bodys, der an die KI geht.
const MAX_BODY: usize = 512 * 1024;
const REDACTED: &str = "***";
/// Antwort-Header, die nie an die KI gehen.
const DROPPED_RESPONSE_HEADERS: [&str; 4] = ["set-cookie", "set-cookie2", "authorization", "proxy-authorization"];

/// Welches Geheimnis eingesetzt wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretSource {
    Password,
    ApiToken,
}

impl SecretSource {
    pub fn key(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::ApiToken => "apiToken",
        }
    }

    /// API-Token hat Vorrang; `None`, wenn weder Token noch Passwort hinterlegt sind.
    pub fn for_entry(e: &Entry) -> Option<Self> {
        if !e.api_token.is_empty() {
            Some(Self::ApiToken)
        } else if !e.password.is_empty() {
            Some(Self::Password)
        } else {
            None
        }
    }
}

/// Zugangsdaten eines Eintrags für genau eine Anfrage.
pub struct Credentials {
    pub username: Zeroizing<String>,
    pub secret: Zeroizing<String>,
    pub source: SecretSource,
    /// Das jeweils andere Geheimnis des Eintrags – wird ebenfalls aus Antworten entfernt.
    pub other: Zeroizing<String>,
}

impl Credentials {
    pub fn from_entry(e: &Entry) -> Option<Self> {
        let source = SecretSource::for_entry(e)?;
        let (secret, other) = match source {
            SecretSource::ApiToken => (&e.api_token, &e.password),
            SecretSource::Password => (&e.password, &e.api_token),
        };
        Some(Self {
            username: Zeroizing::new(e.username.clone()),
            secret: Zeroizing::new(secret.clone()),
            source,
            other: Zeroizing::new(other.clone()),
        })
    }

    fn needles(&self) -> Vec<Zeroizing<String>> {
        needles(&self.username, &[&self.secret, &self.other])
    }
}

/// Geprüfte Anfrage, noch ohne Geheimnis.
#[derive(Debug)]
pub struct Prepared {
    pub method: String,
    pub url: Url,
    pub host: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub auth: AuthLocation,
}

impl Prepared {
    /// Kurzbeschreibung für Dialog und Protokoll (ohne Query, die Daten enthalten kann).
    pub fn summary(&self) -> String {
        format!("{} {}{}", self.method, self.host, self.url.path())
    }
}

/// Prüft eine Anfrage der KI gegen die Freigabe des Eintrags.
pub fn prepare(req: &HttpRequest, policy: &AgentPolicy) -> Result<Prepared, String> {
    let method = req.method.trim().to_ascii_uppercase();
    if !METHODS.contains(&method.as_str()) {
        return Err(format!("Methode {method} ist nicht erlaubt (erlaubt: {})", METHODS.join(", ")));
    }

    let url = Url::parse(req.url.trim()).map_err(|_| "Ungültige URL".to_string())?;
    let host = url.host_str().ok_or("URL ohne Host")?.to_ascii_lowercase();
    match url.scheme() {
        "https" => {}
        "http" if LOOPBACK_HOSTS.contains(&host.as_str()) => {}
        _ => return Err("Nur https-Adressen sind erlaubt (http nur für localhost, 127.0.0.1 und [::1])".into()),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Zugangsdaten in der URL sind nicht erlaubt".into());
    }
    if !policy.allows_host(&host) {
        return Err(format!(
            "Host {host} ist für diesen Eintrag nicht freigegeben (erlaubt: {})",
            policy.hosts.join(", ")
        ));
    }

    let auth = choose_auth(req.auth.as_deref(), &policy.auth)?;

    if req.headers.len() > MAX_HEADERS {
        return Err(format!("Höchstens {MAX_HEADERS} Header erlaubt"));
    }
    let mut headers = Vec::with_capacity(req.headers.len());
    for (name, value) in &req.headers {
        let lower = name.trim().to_ascii_lowercase();
        let token = !lower.is_empty() && lower.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
        if !token {
            return Err(format!("Ungültiger Header-Name: {name}"));
        }
        let secret_header = auth.header_name().is_some_and(|h| h.eq_ignore_ascii_case(&lower));
        if lower == "authorization" || secret_header || FORBIDDEN_HEADERS.contains(&lower.as_str())
        {
            return Err(format!("Header {name} darf nicht gesetzt werden – Remember Key setzt die Anmeldung selbst"));
        }
        if value.contains(['\r', '\n']) {
            return Err(format!("Ungültiger Wert für Header {name}"));
        }
        headers.push((name.trim().to_string(), value.clone()));
    }

    if req.body.is_some() && matches!(method.as_str(), "GET" | "HEAD") {
        return Err(format!("{method}-Anfragen haben keinen Body"));
    }
    check_body_field(&auth, &method, &headers, req.body.as_deref())?;

    Ok(Prepared { method, url, host, headers, body: req.body.clone(), auth })
}

/// Content-Type, den ein Body mit eingesetztem Feld haben muss.
fn body_content_type(auth: &AuthLocation) -> Option<&'static str> {
    match auth {
        AuthLocation::FormField { .. } => Some("application/x-www-form-urlencoded"),
        AuthLocation::JsonField { .. } => Some("application/json"),
        _ => None,
    }
}

/// Bei Feldern im Body: passende Methode und passender Content-Type; der Body der KI darf
/// das Feld nicht selbst enthalten.
fn check_body_field(auth: &AuthLocation, method: &str, headers: &[(String, String)], body: Option<&str>) -> Result<(), String> {
    let (Some(field), Some(ct)) = (auth.body_field(), body_content_type(auth)) else { return Ok(()) };
    if !matches!(method, "POST" | "PUT" | "PATCH") {
        return Err(format!("Einsetzen in ein Body-Feld geht nur mit POST, PUT oder PATCH, nicht mit {method}"));
    }
    if let Some((_, v)) = headers.iter().find(|(n, _)| n.eq_ignore_ascii_case("content-type")) {
        if !v.trim().to_ascii_lowercase().starts_with(ct) {
            return Err(format!("Content-Type muss {ct} sein (oder weglassen)"));
        }
    }
    let Some(body) = body.filter(|b| !b.trim().is_empty()) else { return Ok(()) };
    match auth {
        AuthLocation::FormField { .. } => {
            if url::form_urlencoded::parse(body.as_bytes()).any(|(k, _)| k == field) {
                return Err(format!("Das Feld {field} setzt Remember Key selbst – bitte aus dem Body entfernen"));
            }
        }
        _ => {
            let value: Value = serde_json::from_str(body).map_err(|_| "Body muss gültiges JSON sein".to_string())?;
            let obj = value.as_object().ok_or("JSON-Body muss ein Objekt sein")?;
            if obj.contains_key(field) {
                return Err(format!("Das Feld {field} setzt Remember Key selbst – bitte aus dem Body entfernen"));
            }
        }
    }
    Ok(())
}

/// Body mit eingesetztem Geheimnis (nur bei Body-Feldern).
fn body_with_secret(auth: &AuthLocation, body: Option<&str>, secret: &str) -> Result<Option<Zeroizing<String>>, String> {
    let base = body.map(str::trim).filter(|b| !b.is_empty());
    match auth {
        AuthLocation::FormField { name } => {
            let mut ser = url::form_urlencoded::Serializer::new(base.unwrap_or_default().to_string());
            ser.append_pair(name, secret);
            Ok(Some(Zeroizing::new(ser.finish())))
        }
        AuthLocation::JsonField { name } => {
            let mut obj = match base {
                Some(b) => serde_json::from_str::<Map<String, Value>>(b).map_err(|_| "JSON-Body muss ein Objekt sein")?,
                None => Map::new(),
            };
            obj.insert(name.clone(), Value::String(secret.to_string()));
            let text = Zeroizing::new(serde_json::to_string(&obj).map_err(|e| e.to_string())?);
            if let Some(Value::String(s)) = obj.get_mut(name) {
                zeroize::Zeroize::zeroize(s);
            }
            Ok(Some(text))
        }
        _ => Ok(None),
    }
}

fn choose_auth(requested: Option<&str>, allowed: &[AuthLocation]) -> Result<AuthLocation, String> {
    let keys = || allowed.iter().map(AuthLocation::key).collect::<Vec<_>>().join(", ");
    if allowed.is_empty() {
        return Err("Für diesen Eintrag ist das Einsetzen in HTTP-Anfragen nicht freigegeben".into());
    }
    match requested.map(str::trim).filter(|s| !s.is_empty()) {
        Some(r) => allowed
            .iter()
            .find(|a| a.key().eq_ignore_ascii_case(r))
            .cloned()
            .ok_or_else(|| format!("Einsetz-Stelle {r} ist nicht freigegeben (erlaubt: {})", keys())),
        None if allowed.len() == 1 => Ok(allowed[0].clone()),
        None => Err(format!("Bitte `auth` angeben (erlaubt: {})", keys())),
    }
}

/// Wert des Auth-Headers für die gewählte Stelle.
fn auth_value(auth: &AuthLocation, username: &str, password: &str) -> Zeroizing<String> {
    Zeroizing::new(match auth {
        AuthLocation::Bearer => format!("Bearer {password}"),
        AuthLocation::Basic => {
            let pair = Zeroizing::new(format!("{username}:{password}"));
            format!("Basic {}", BASE64.encode(pair.as_bytes()))
        }
        AuthLocation::Header { .. } | AuthLocation::FormField { .. } | AuthLocation::JsonField { .. } => {
            password.to_string()
        }
    })
}

/// Führt die Anfrage aus und gibt die bereinigte Antwort zurück.
pub fn execute(p: &Prepared, c: &Credentials) -> Result<Value, String> {
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("RememberKey/", env!("CARGO_PKG_VERSION")))
        .build();
    let mut request = agent.request_url(&p.method, &p.url);
    for (name, value) in &p.headers {
        request = request.set(name, value);
    }
    if let Some(header) = p.auth.header_name() {
        let value = auth_value(&p.auth, &c.username, &c.secret);
        request = request.set(header, &value);
    }
    let secret_body = body_with_secret(&p.auth, p.body.as_deref(), &c.secret)?;
    if let Some(ct) = body_content_type(&p.auth) {
        if !p.headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("content-type")) {
            request = request.set("Content-Type", ct);
        }
    }

    let result = match (&secret_body, &p.body) {
        (Some(body), _) => request.send_string(body),
        (None, Some(body)) => request.send_string(body),
        (None, None) => request.call(),
    };
    drop(secret_body);
    let response = match result {
        Ok(r) | Err(ureq::Error::Status(_, r)) => r,
        Err(ureq::Error::Transport(t)) => {
            return Err(redact(&format!("Verbindung fehlgeschlagen: {t}"), &c.needles()));
        }
    };

    let needles = c.needles();
    let status = response.status();
    let status_text = response.status_text().to_string();
    let mut headers = Map::new();
    for name in response.headers_names() {
        if DROPPED_RESPONSE_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
            continue;
        }
        let values: Vec<String> = response.all(&name).iter().map(|v| redact(v, &needles)).collect();
        headers.insert(name, Value::String(values.join(", ")));
    }

    let mut raw = Vec::new();
    response
        .into_reader()
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|e| format!("Antwort konnte nicht gelesen werden: {e}"))?;
    let truncated = raw.len() > MAX_BODY;
    raw.truncate(MAX_BODY);
    let raw = Zeroizing::new(raw);
    let body = match std::str::from_utf8(&raw) {
        Ok(text) => redact(text, &needles),
        // Abgeschnitten mitten in einem Zeichen: gültigen Anfang verwenden.
        Err(e) if truncated && e.error_len().is_none() => redact(
            std::str::from_utf8(&raw[..e.valid_up_to()]).unwrap_or_default(),
            &needles,
        ),
        Err(_) => format!("[Binärdaten, {} Bytes]", raw.len()),
    };

    let mut out = json!({
        "status": status,
        "statusText": status_text,
        "headers": headers,
        "body": body,
        "truncated": truncated,
        // Geheimnisfreie Diagnose: wo Remember Key die Zugangsdaten eingesetzt hat.
        "injected": injected(&p.auth, c),
    });
    if (300..400).contains(&status) {
        out["note"] = json!("Weiterleitungen werden nicht verfolgt. Bei Bedarf die Location-Adresse erneut anfragen.");
    }
    Ok(out)
}

/// Beschreibt die Einsetz-Stelle, ohne etwas über das Geheimnis zu verraten.
fn injected(auth: &AuthLocation, c: &Credentials) -> Value {
    let mut v = json!({ "location": auth.key(), "secret": c.source.key() });
    if let Some(h) = auth.header_name() {
        v["header"] = json!(h);
    }
    if let Some(f) = auth.body_field() {
        v["field"] = json!(f);
    }
    if *auth == AuthLocation::Basic {
        v["withUsername"] = json!(!c.username.is_empty());
    }
    v["hint"] = json!(match c.source {
        SecretSource::Password => {
            "Eingesetzt wurde das Login-Passwort. Bei 401/403 erwarten viele APIs stattdessen einen API-Token – \
             den Nutzer bitten, ihn im Eintrag unter „API-Token“ zu hinterlegen; außerdem Host und Einsetz-Stelle prüfen."
        }
        SecretSource::ApiToken => {
            "Eingesetzt wurde der API-Token. Bei 401/403 Token, Host (z. B. Mandanten-Adresse) und Einsetz-Stelle \
             vom Nutzer prüfen lassen."
        }
    });
    v
}

/// Alle Formen, in denen das Geheimnis in einer Antwort auftauchen könnte – längste zuerst.
pub fn needles(username: &str, values: &[&str]) -> Vec<Zeroizing<String>> {
    let mut secrets: Vec<Zeroizing<String>> = Vec::new();
    for v in values.iter().filter(|v| !v.is_empty()) {
        secrets.push(Zeroizing::new(v.to_string()));
        if !username.is_empty() {
            secrets.push(Zeroizing::new(format!("{username}:{v}")));
        }
    }
    let mut out: Vec<Zeroizing<String>> = Vec::new();
    for s in secrets.iter().filter(|s| !s.is_empty()) {
        let b = s.as_bytes();
        let json = serde_json::to_string(s.as_str()).unwrap_or_default();
        for v in [
            s.to_string(),
            BASE64.encode(b),
            BASE64_NOPAD.encode(b),
            BASE64URL.encode(b),
            BASE64URL_NOPAD.encode(b),
            HEXLOWER.encode(b),
            HEXUPPER.encode(b),
            percent_encode(s, false),
            percent_encode(s, true),
            url::form_urlencoded::byte_serialize(b).collect(),
            json[1..json.len().saturating_sub(1)].to_string(),
            html_escape(s),
        ] {
            if !v.is_empty() && !out.iter().any(|o| **o == v) {
                out.push(Zeroizing::new(v));
            }
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.len()));
    out
}

pub fn redact(text: &str, needles: &[Zeroizing<String>]) -> String {
    let mut out = text.to_string();
    for n in needles {
        if out.contains(n.as_str()) {
            out = out.replace(n.as_str(), REDACTED);
        }
    }
    out
}

fn percent_encode(s: &str, lower: bool) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else if lower {
            out.push_str(&format!("%{b:02x}"));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn policy(auth: Vec<AuthLocation>) -> AgentPolicy {
        AgentPolicy { enabled: true, hosts: vec!["api.github.com".into(), "*.example.com".into()], auth, session_minutes: 0, fill_login: false }
    }

    fn req(url: &str) -> HttpRequest {
        HttpRequest {
            entry_id: "1".into(),
            method: "get".into(),
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            auth: None,
        }
    }

    #[test]
    fn accepts_allowed_request() {
        let p = prepare(&req("https://api.github.com/user?x=1"), &policy(vec![AuthLocation::Bearer])).unwrap();
        assert_eq!(p.method, "GET");
        assert_eq!(p.auth, AuthLocation::Bearer);
        assert_eq!(p.summary(), "GET api.github.com/user");
        assert!(prepare(&req("https://a.b.example.com/"), &policy(vec![AuthLocation::Bearer])).is_ok());
    }

    #[test]
    fn http_only_for_loopback() {
        let pol = AgentPolicy {
            enabled: true,
            hosts: vec!["localhost".into(), "127.0.0.1".into(), "intranet.example.com".into()],
            auth: vec![AuthLocation::Basic],
            session_minutes: 0,
            fill_login: false,
        };
        assert!(prepare(&req("http://localhost:7781/api"), &pol).is_ok());
        assert!(prepare(&req("http://127.0.0.1/"), &pol).is_ok());
        assert!(prepare(&req("http://intranet.example.com/"), &pol).is_err());
        // Loopback ersetzt nicht die Host-Freigabe.
        assert!(prepare(&req("http://localhost/"), &policy(vec![AuthLocation::Bearer])).is_err());
    }

    fn entry(password: &str, api_token: &str) -> Entry {
        Entry {
            id: "1".into(),
            title: "T".into(),
            username: "me".into(),
            password: password.into(),
            api_token: api_token.into(),
            url: String::new(),
            notes: String::new(),
            totp: None,
            folder_id: None,
            created_at: 0,
            updated_at: 0,
            agent: None,
        }
    }

    #[test]
    fn api_token_takes_precedence() {
        let c = Credentials::from_entry(&entry("login-pw", "tok-123")).unwrap();
        assert_eq!(c.source, SecretSource::ApiToken);
        assert_eq!(*c.secret, "tok-123");
        let c = Credentials::from_entry(&entry("login-pw", "")).unwrap();
        assert_eq!(c.source, SecretSource::Password);
        assert!(Credentials::from_entry(&entry("", "")).is_none());
        // Beide Geheimnisse werden aus Antworten entfernt.
        let c = Credentials::from_entry(&entry("login-pw", "tok-123")).unwrap();
        let out = redact("a tok-123 b login-pw c", &c.needles());
        assert!(!out.contains("tok-123") && !out.contains("login-pw"));
    }

    #[test]
    fn diagnostics_reveal_no_secret() {
        let c = Credentials::from_entry(&entry("pw-geheim", "")).unwrap();
        let v = injected(&AuthLocation::Basic, &c);
        assert_eq!(v["location"], "basic");
        assert_eq!(v["header"], "Authorization");
        assert_eq!(v["secret"], "password");
        assert_eq!(v["withUsername"], true);
        let c = Credentials::from_entry(&entry("", "tok-geheim")).unwrap();
        let v = injected(&AuthLocation::Header { name: "AuthenticationToken".into() }, &c);
        assert_eq!(v["header"], "AuthenticationToken");
        assert_eq!(v["secret"], "apiToken");
        assert!(v.get("withUsername").is_none());
        let text = v.to_string();
        assert!(!text.contains("geheim"));
    }

    #[test]
    fn rejects_wrong_scheme_host_and_userinfo() {
        let pol = policy(vec![AuthLocation::Bearer]);
        for url in [
            "http://api.github.com/",
            "https://github.com/",
            "https://api.github.com.evil.com/",
            "https://example.com/",
            "https://user:pw@api.github.com/",
            "ftp://api.github.com/",
            "kein-url",
        ] {
            assert!(prepare(&req(url), &pol).is_err(), "{url}");
        }
    }

    #[test]
    fn rejects_auth_headers_and_bad_methods() {
        let pol = policy(vec![AuthLocation::Header { name: "X-Api-Key".into() }]);
        for h in ["Authorization", "cookie", "X-API-KEY", "Host", "Bad Name"] {
            let mut r = req("https://api.github.com/");
            r.headers.insert(h.into(), "x".into());
            assert!(prepare(&r, &pol).is_err(), "{h}");
        }
        let mut r = req("https://api.github.com/");
        r.headers.insert("Accept".into(), "a\r\nX-Evil: 1".into());
        assert!(prepare(&r, &pol).is_err());
        let mut r = req("https://api.github.com/");
        r.method = "TRACE".into();
        assert!(prepare(&r, &pol).is_err());
        let mut r = req("https://api.github.com/");
        r.body = Some("{}".into());
        assert!(prepare(&r, &pol).is_err());
    }

    #[test]
    fn auth_selection() {
        assert!(prepare(&req("https://api.github.com/"), &policy(vec![])).is_err());
        let both = policy(vec![AuthLocation::Bearer, AuthLocation::Basic]);
        assert!(prepare(&req("https://api.github.com/"), &both).is_err());
        let mut r = req("https://api.github.com/");
        r.auth = Some("BASIC".into());
        assert_eq!(prepare(&r, &both).unwrap().auth, AuthLocation::Basic);
        r.auth = Some("header:X".into());
        assert!(prepare(&r, &both).is_err());
    }

    #[test]
    fn body_fields_are_checked_and_filled() {
        let form = policy(vec![AuthLocation::FormField { name: "password".into() }]);
        let mut r = req("https://api.github.com/login");
        assert!(prepare(&r, &form).is_err(), "GET mit Body-Feld");
        r.method = "POST".into();
        r.body = Some("user=me&password=x".into());
        assert!(prepare(&r, &form).is_err(), "Feld schon im Body");
        r.body = Some("user=me".into());
        r.headers.insert("Content-Type".into(), "application/json".into());
        assert!(prepare(&r, &form).is_err(), "falscher Content-Type");
        r.headers.clear();
        let p = prepare(&r, &form).unwrap();
        let body = body_with_secret(&p.auth, p.body.as_deref(), "a&b=c").unwrap().unwrap();
        assert_eq!(body.as_str(), "user=me&password=a%26b%3Dc");

        let json_pol = policy(vec![AuthLocation::JsonField { name: "token".into() }]);
        let mut r = req("https://api.github.com/login");
        r.method = "POST".into();
        r.body = Some(r#"{"user":"me","token":"x"}"#.into());
        assert!(prepare(&r, &json_pol).is_err());
        r.body = Some("[1]".into());
        assert!(prepare(&r, &json_pol).is_err());
        r.body = Some(r#"{"user":"me"}"#.into());
        let p = prepare(&r, &json_pol).unwrap();
        let body = body_with_secret(&p.auth, p.body.as_deref(), "geheim\"1").unwrap().unwrap();
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["user"], "me");
        assert_eq!(v["token"], "geheim\"1");
        // Ohne Body der KI: nur das Feld
        assert_eq!(body_with_secret(&p.auth, None, "s").unwrap().unwrap().as_str(), r#"{"token":"s"}"#);
    }

    #[test]
    fn auth_values() {
        assert_eq!(*auth_value(&AuthLocation::Bearer, "u", "pw"), "Bearer pw");
        assert_eq!(*auth_value(&AuthLocation::Basic, "u", "pw"), "Basic dTpwdw==");
        assert_eq!(*auth_value(&AuthLocation::Header { name: "X".into() }, "u", "pw"), "pw");
    }

    #[test]
    fn redacts_secret_in_all_encodings() {
        let pw = "Pa$$ w/\"rd<1>";
        let n = needles("me@x.de", &[pw]);
        let pair = format!("me@x.de:{pw}");
        let samples = [
            pw.to_string(),
            BASE64.encode(pw.as_bytes()),
            BASE64URL_NOPAD.encode(pair.as_bytes()),
            HEXLOWER.encode(pw.as_bytes()),
            HEXUPPER.encode(pw.as_bytes()),
            percent_encode(pw, false),
            url::form_urlencoded::byte_serialize(pw.as_bytes()).collect(),
            serde_json::to_string(pw).unwrap(),
            html_escape(pw),
            format!("Basic {}", BASE64.encode(pair.as_bytes())),
        ];
        for s in samples {
            let text = format!("vorher {s} nachher");
            let out = redact(&text, &n);
            assert!(out.contains(REDACTED), "{s}");
            assert!(!out.contains(pw), "{s}");
        }
        assert_eq!(redact("nichts Geheimes", &n), "nichts Geheimes");
    }

    /// Echo-Dienst spiegelt die gesendeten Header zurück – genau der Fall, in dem das
    /// Geheimnis in der Antwort auftauchen würde. Netzwerkzugriff, daher nur auf Anfrage:
    /// `cargo test -- --ignored echo_service`
    #[test]
    #[ignore]
    fn echo_service_reflection_is_redacted() {
        let pw = "dummy-Geheimnis-7f3a";
        for auth in [
            AuthLocation::Bearer,
            AuthLocation::Basic,
            AuthLocation::Header { name: "X-Api-Key".into() },
            AuthLocation::FormField { name: "password".into() },
            AuthLocation::JsonField { name: "password".into() },
        ] {
            let body_field = auth.body_field().is_some();
            let pol = AgentPolicy { enabled: true, hosts: vec!["httpbin.org".into()], auth: vec![auth], session_minutes: 0, fill_login: false };
            let mut r = req("https://httpbin.org/anything?probe=1");
            if body_field {
                r.method = "POST".into();
            }
            let p = prepare(&r, &pol).unwrap();
            let mut e = entry(pw, "");
            e.username = "tester".into();
            let out = execute(&p, &Credentials::from_entry(&e).unwrap()).unwrap();
            let text = out.to_string();
            assert_eq!(out["status"], 200, "{text}");
            assert!(text.contains(REDACTED), "{text}");
            assert!(!text.contains(pw) && !text.contains(&BASE64.encode(format!("tester:{pw}").as_bytes())), "{text}");
        }
    }

    #[test]
    fn needles_longest_first() {
        let n = needles("u", &["abcd"]);
        assert!(n.windows(2).all(|w| w[0].len() >= w[1].len()));
        // Benutzer:Passwort als Ganzes, nicht nur das Passwort darin.
        assert_eq!(redact("u:abcd", &n), REDACTED);
    }
}
