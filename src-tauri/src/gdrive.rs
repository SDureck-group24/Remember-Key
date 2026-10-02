//! Minimaler Google-Drive-Client (REST v3) für den versteckten App-Datenordner.
//!
//! - Anmeldung: OAuth 2.0 für Desktop-Apps mit Loopback-Redirect und PKCE (RFC 8252/7636).
//! - Berechtigung: nur `drive.appdata` – die App sieht ausschließlich ihren eigenen,
//!   für den Nutzer unsichtbaren Ordner, nicht die übrigen Drive-Dateien.
//! - Hochgeladen wird nur die bereits verschlüsselte Tresor-Datei.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use data_encoding::{BASE64URL_NOPAD, HEXLOWER};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::error::{Error, Result};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";
const FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";
const UPLOAD_URL: &str = "https://www.googleapis.com/upload/drive/v3/files";
const FIELDS: &str = "id,version,modifiedTime";
const REMOTE_NAME: &str = "vault.rk";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct Client {
    pub client_id: String,
    /// Bei Desktop-Clients laut Google nicht vertraulich, wird aber benötigt.
    pub client_secret: String,
}

pub struct Tokens {
    pub access: Zeroizing<String>,
    pub expires: Instant,
    pub refresh: Option<Zeroizing<String>>,
}

impl Tokens {
    pub fn valid(&self) -> bool {
        self.expires > Instant::now() + Duration::from_secs(60)
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFile {
    pub id: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Deserialize)]
struct FileList {
    files: Vec<RemoteFile>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
}

fn http_err(e: ureq::Error) -> Error {
    match e {
        ureq::Error::Status(401, _) => Error::SyncAuth,
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            if body.contains("invalid_grant") {
                return Error::SyncAuth;
            }
            let short: String = body.chars().take(200).collect();
            Error::Sync(format!("Anfrage abgelehnt ({code}): {short}"))
        }
        ureq::Error::Transport(t) => Error::Offline(t.kind().to_string()),
    }
}

fn json<T: serde::de::DeserializeOwned>(resp: ureq::Response) -> Result<T> {
    resp.into_json().map_err(|e| Error::Sync(format!("Unerwartete Antwort: {e}")))
}

fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buf);
    BASE64URL_NOPAD.encode(&buf)
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

// ---------- OAuth ----------

/// Öffnet den Google-Login im Browser und wartet auf den Redirect an 127.0.0.1.
pub fn authorize(client: &Client, open_browser: impl FnOnce(&str) -> Result<()>) -> Result<Tokens> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let redirect = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let verifier = Zeroizing::new(random_token(48));
    let challenge = BASE64URL_NOPAD.encode(&Sha256::digest(verifier.as_bytes()));
    let state = random_token(16);

    let mut url = url::Url::parse(AUTH_URL).expect("gültige URL");
    url.query_pairs_mut()
        .append_pair("client_id", &client.client_id)
        .append_pair("redirect_uri", &redirect)
        .append_pair("response_type", "code")
        .append_pair("scope", SCOPE)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    open_browser(url.as_str())?;

    let code = wait_for_code(&listener, &state)?;
    let resp = agent()
        .post(TOKEN_URL)
        .send_form(&[
            ("code", code.as_str()),
            ("client_id", &client.client_id),
            ("client_secret", &client.client_secret),
            ("redirect_uri", &redirect),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier.as_str()),
        ])
        .map_err(http_err)?;
    let t: TokenResponse = json(resp)?;
    let refresh = t
        .refresh_token
        .ok_or_else(|| Error::Sync("Google hat kein Refresh-Token geliefert".into()))?;
    Ok(Tokens {
        access: Zeroizing::new(t.access_token),
        expires: Instant::now() + Duration::from_secs(t.expires_in),
        refresh: Some(Zeroizing::new(refresh)),
    })
}

fn respond(mut stream: TcpStream, status: &str, title: &str, text: &str) {
    let body = format!(
        "<!doctype html><html lang=\"de\"><meta charset=\"utf-8\"><title>Remember Key</title>\
         <body style=\"font-family:system-ui;background:#161826;color:#e9e9ed;display:grid;place-items:center;height:100vh;margin:0\">\
         <div style=\"max-width:420px\"><h2 style=\"font-weight:500\">{title}</h2><p style=\"opacity:.7\">{text}</p></div></body></html>"
    );
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn wait_for_code(listener: &TcpListener, expected_state: &str) -> Result<Zeroizing<String>> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    loop {
        if Instant::now() > deadline {
            return Err(Error::Sync("Zeitüberschreitung bei der Google-Anmeldung".into()));
        }
        let stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut line = String::new();
        if BufReader::new(&stream).read_line(&mut line).is_err() {
            continue;
        }
        // "GET /?code=…&state=… HTTP/1.1"
        let Some(path) = line.split_whitespace().nth(1) else { continue };
        let Ok(url) = url::Url::parse(&format!("http://127.0.0.1{path}")) else { continue };
        let param = |k: &str| url.query_pairs().find(|(key, _)| key == k).map(|(_, v)| v.into_owned());

        if let Some(err) = param("error") {
            respond(stream, "200 OK", "Anmeldung abgebrochen", "Sie können dieses Fenster schließen.");
            return Err(Error::Sync(format!("Anmeldung abgebrochen ({err})")));
        }
        let Some(code) = param("code") else {
            respond(stream, "404 Not Found", "Nicht gefunden", "");
            continue;
        };
        if param("state").as_deref() != Some(expected_state) {
            respond(stream, "400 Bad Request", "Ungültige Anfrage", "Bitte starten Sie die Anmeldung erneut.");
            return Err(Error::Sync("Ungültige Antwort von Google (state stimmt nicht)".into()));
        }
        respond(
            stream,
            "200 OK",
            "Verbunden mit Google Drive",
            "Remember Key ist jetzt verbunden. Sie können dieses Fenster schließen.",
        );
        return Ok(Zeroizing::new(code));
    }
}

pub fn refresh(client: &Client, refresh_token: &str) -> Result<Tokens> {
    let resp = agent()
        .post(TOKEN_URL)
        .send_form(&[
            ("client_id", client.client_id.as_str()),
            ("client_secret", &client.client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .map_err(http_err)?;
    let t: TokenResponse = json(resp)?;
    Ok(Tokens {
        access: Zeroizing::new(t.access_token),
        expires: Instant::now() + Duration::from_secs(t.expires_in),
        refresh: t.refresh_token.map(Zeroizing::new),
    })
}

/// Widerruft das Token bei Google (Fehler werden ignoriert – lokal wird ohnehin gelöscht).
pub fn revoke(token: &str) {
    let _ = agent().post(REVOKE_URL).send_form(&[("token", token)]);
}

// ---------- Dateien ----------

pub fn find(token: &str) -> Result<Option<RemoteFile>> {
    Ok(find_all(token)?.into_iter().next())
}

/// Alle Tresor-Dateien im App-Ordner, neueste zuerst.
pub fn find_all(token: &str) -> Result<Vec<RemoteFile>> {
    let resp = agent()
        .get(FILES_URL)
        .set("Authorization", &bearer(token))
        .query("spaces", "appDataFolder")
        .query("q", &format!("name = '{REMOTE_NAME}' and trashed = false"))
        .query("fields", &format!("files({FIELDS})"))
        .query("orderBy", "modifiedTime desc")
        .query("pageSize", "10")
        .call()
        .map_err(http_err)?;
    Ok(json::<FileList>(resp)?.files)
}

/// Löscht eine Datei endgültig – samt allen gespeicherten Versionen.
pub fn delete(token: &str, id: &str) -> Result<()> {
    match agent().delete(&format!("{FILES_URL}/{id}")).set("Authorization", &bearer(token)).call() {
        Ok(_) | Err(ureq::Error::Status(404, _)) => Ok(()),
        Err(e) => Err(http_err(e)),
    }
}

pub fn metadata(token: &str, id: &str) -> Result<Option<RemoteFile>> {
    match agent()
        .get(&format!("{FILES_URL}/{id}"))
        .set("Authorization", &bearer(token))
        .query("fields", FIELDS)
        .call()
    {
        Ok(resp) => Ok(Some(json(resp)?)),
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(e) => Err(http_err(e)),
    }
}

pub fn download(token: &str, id: &str) -> Result<Vec<u8>> {
    let resp = agent()
        .get(&format!("{FILES_URL}/{id}"))
        .set("Authorization", &bearer(token))
        .query("alt", "media")
        .call()
        .map_err(http_err)?;
    let mut buf = Vec::new();
    resp.into_reader().take(MAX_DOWNLOAD).read_to_end(&mut buf)?;
    Ok(buf)
}

pub fn create(token: &str, bytes: &[u8]) -> Result<RemoteFile> {
    let mut b = [0u8; 12];
    OsRng.fill_bytes(&mut b);
    let boundary = format!("remember-key-{}", HEXLOWER.encode(&b));
    let meta = format!(r#"{{"name":"{REMOTE_NAME}","parents":["appDataFolder"]}}"#);
    let mut body = Vec::with_capacity(bytes.len() + 512);
    write!(
        body,
        "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n\
         --{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n"
    )?;
    body.extend_from_slice(bytes);
    write!(body, "\r\n--{boundary}--\r\n")?;

    let resp = agent()
        .post(UPLOAD_URL)
        .set("Authorization", &bearer(token))
        .set("Content-Type", &format!("multipart/related; boundary={boundary}"))
        .query("uploadType", "multipart")
        .query("fields", FIELDS)
        .send_bytes(&body)
        .map_err(http_err)?;
    json(resp)
}

pub fn update(token: &str, id: &str, bytes: &[u8]) -> Result<RemoteFile> {
    let resp = agent()
        .request("PATCH", &format!("{UPLOAD_URL}/{id}"))
        .set("Authorization", &bearer(token))
        .set("Content-Type", "application/octet-stream")
        .query("uploadType", "media")
        .query("fields", FIELDS)
        .send_bytes(bytes)
        .map_err(http_err)?;
    json(resp)
}
