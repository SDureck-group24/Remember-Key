//! Protokoll zwischen der MCP-Brücke (`remember-key-mcp.exe`) und der App.
//!
//! Transport: Named Pipe `\\.\pipe\com.rememberkey.agent.<Benutzer-SID>`, nur für den
//! angemeldeten Benutzer zugänglich, eine Anfrage pro Verbindung. Nachrichten sind JSON
//! mit vorangestellter Länge (u32, little-endian).
//!
//! Die Antworten enthalten nie Geheimnisse (Passwörter, Notizen, TOTP-Schlüssel oder -Codes).

use std::collections::BTreeMap;
use std::io::{self, Read, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Obergrenze für eine einzelne Nachricht.
pub const MAX_MESSAGE: usize = 1024 * 1024;

/// Dateiname der Brücke; liegt im selben Verzeichnis wie die App.
pub const BRIDGE_EXE: &str = "remember-key-mcp.exe";

/// Native-Messaging-Host für die Browser-Erweiterung; liegt ebenfalls neben der App.
pub const BROWSER_HOST_EXE: &str = "remember-key-browser.exe";

/// Name des Native-Messaging-Hosts (Registry und Erweiterung).
pub const NATIVE_HOST_NAME: &str = "com.rememberkey.browser";

/// ID der Erweiterung in Chrome/Edge (aus dem `key` in extension/manifest.json abgeleitet).
pub const CHROME_EXTENSION_ID: &str = "hiakhacjfkmcfnbalgiplidknigomhaf";

/// ID der Erweiterung in Firefox/Zen (`browser_specific_settings.gecko.id`).
pub const GECKO_EXTENSION_ID: &str = "remember-key@rememberkey.app";

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    ListEntries {
        #[serde(default)]
        query: Option<String>,
    },
    HttpRequest(HttpRequest),
    FillLogin(FillLogin),
    /// Der Native-Messaging-Host meldet einen Browser an und hält die Verbindung offen;
    /// darüber schickt die App anschließend `BrowserCommand`s.
    RegisterBrowser { browser: String },
    /// Ergebnis der Erweiterung zu einem `BrowserCommand`.
    BrowserResult { id: u64, result: Response },
}

/// Login im Browser ausfüllen und absenden lassen.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillLogin {
    pub entry_id: String,
    /// Optional: Adresse oder Host des Tabs, falls mehrere passende Tabs offen sind.
    #[serde(default)]
    pub url: Option<String>,
}

/// Befehl der App an die Browser-Erweiterung (über den Native-Messaging-Host).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BrowserCommand {
    #[serde(rename_all = "camelCase")]
    Fill {
        id: u64,
        /// Hosts der Freigabe; die Erweiterung füllt nur auf passenden Seiten aus.
        hosts: Vec<String>,
        /// Nur Tabs mit genau diesem Host (aus `FillLogin::url`).
        host_hint: Option<String>,
        username: String,
        password: String,
        /// Aktueller 2FA-Code, falls der Eintrag einen TOTP-Schlüssel hat. Die Erweiterung
        /// setzt ihn nur ein, wenn die Seite nach einem Code fragt.
        #[serde(default)]
        otp: Option<String>,
    },
}

/// HTTPS-Anfrage, in die die App die Zugangsdaten eines Eintrags selbst einsetzt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRequest {
    pub entry_id: String,
    #[serde(default = "default_method")]
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
    /// Einsetz-Stelle (`bearer`, `basic`, `header:<Name>`); optional, wenn nur eine erlaubt ist.
    #[serde(default)]
    pub auth: Option<String>,
    /// Cookie-Sitzung: `new` meldet an und behält die Cookies der Antwort in der App, eine
    /// Sitzungs-ID schickt sie bei Folgeanfragen mit. Die Cookies selbst sieht die KI nie.
    #[serde(default)]
    pub session: Option<String>,
    /// Sitzung nach dieser Anfrage beenden (z. B. beim Logout).
    #[serde(default)]
    pub end_session: bool,
}

fn default_method() -> String {
    "GET".into()
}

/// Erfolg mit Ergebnis-JSON oder eine Fehlermeldung, die an die KI weitergegeben wird.
pub type Response = Result<serde_json::Value, String>;

/// Eintrag, wie ihn die KI sieht.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEntry {
    pub id: String,
    pub title: String,
    pub username: String,
    /// Hosts, an die die Zugangsdaten gebunden sind.
    pub hosts: Vec<String>,
    /// Erlaubte Einsetz-Stellen für `http_request` (`bearer`, `basic`, `header:<Name>`).
    pub auth: Vec<String>,
    /// Login darf per Browser-Erweiterung ausgefüllt werden (`fill_login`).
    pub fill_login: bool,
    /// Was eingesetzt wird: `apiToken` (hat Vorrang) oder `password`; `None` = nichts hinterlegt.
    pub secret: Option<String>,
    pub has_totp: bool,
}

pub fn write_message(w: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_MESSAGE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Nachricht zu groß"));
    }
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(&bytes)?;
    w.flush()
}

pub fn read_message<T: DeserializeOwned>(r: &mut impl Read) -> io::Result<T> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_MESSAGE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Nachricht zu groß"));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}

/// Gleicher Pfad unter Windows (Groß-/Kleinschreibung egal).
pub fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

#[cfg(windows)]
pub mod pipe {
    //! Named Pipe mit Zugriff nur für den aktuellen Benutzer.

    use std::ffi::OsString;
    use std::io::{self, Read, Write};
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use std::ptr;
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE,
        HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FlushFileBuffers, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, OPEN_EXISTING,
        PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
        GetNamedPipeServerProcessId, PeekNamedPipe, WaitNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const BUFFER: u32 = 64 * 1024;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn last_error() -> io::Error {
        io::Error::last_os_error()
    }

    /// SID des angemeldeten Benutzers als Text (`S-1-5-21-…`).
    pub fn user_sid() -> io::Result<&'static str> {
        static SID: OnceLock<String> = OnceLock::new();
        if let Some(s) = SID.get() {
            return Ok(s);
        }
        let sid = unsafe {
            let mut token: HANDLE = ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(last_error());
            }
            let mut len = 0u32;
            GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut len);
            // u64-Puffer für die Ausrichtung von TOKEN_USER.
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            let ok = GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len);
            CloseHandle(token);
            if ok == 0 {
                return Err(last_error());
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut s: *mut u16 = ptr::null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut s) == 0 {
                return Err(last_error());
            }
            let n = (0..).take_while(|&i| *s.add(i) != 0).count();
            let out = OsString::from_wide(std::slice::from_raw_parts(s, n)).to_string_lossy().into_owned();
            LocalFree(s.cast());
            out
        };
        Ok(SID.get_or_init(|| sid))
    }

    pub fn pipe_name() -> io::Result<String> {
        Ok(format!(r"\\.\pipe\com.rememberkey.agent.{}", user_sid()?))
    }

    /// Vollständiger Pfad der ausführbaren Datei eines Prozesses.
    pub fn process_image(pid: u32) -> io::Result<PathBuf> {
        unsafe {
            let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if p.is_null() {
                return Err(last_error());
            }
            let mut buf = vec![0u16; 32 * 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
            CloseHandle(p);
            if ok == 0 {
                return Err(last_error());
            }
            Ok(PathBuf::from(OsString::from_wide(&buf[..len as usize])))
        }
    }

    pub struct Pipe {
        handle: HANDLE,
        server: bool,
    }

    // Das Handle gehört exklusiv diesem Objekt.
    unsafe impl Send for Pipe {}

    impl Pipe {
        /// Neue Server-Instanz. `first` verhindert, dass ein anderer Prozess den Namen
        /// vorher belegt hat (dann schlägt das Anlegen fehl).
        pub fn create_server(first: bool) -> io::Result<Self> {
            let name = wide(&pipe_name()?);
            // Nur der aktuelle Benutzer, keine geerbten Einträge.
            let sddl = wide(&format!("D:P(A;;GA;;;{})", user_sid()?));
            unsafe {
                let mut sd: PSECURITY_DESCRIPTOR = ptr::null_mut();
                if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut sd,
                    ptr::null_mut(),
                ) == 0
                {
                    return Err(last_error());
                }
                let sa = SECURITY_ATTRIBUTES {
                    nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                    lpSecurityDescriptor: sd,
                    bInheritHandle: 0,
                };
                let mut open_mode = PIPE_ACCESS_DUPLEX;
                if first {
                    open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
                }
                let handle = CreateNamedPipeW(
                    name.as_ptr(),
                    open_mode,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    PIPE_UNLIMITED_INSTANCES,
                    BUFFER,
                    BUFFER,
                    0,
                    &sa,
                );
                LocalFree(sd);
                if handle == INVALID_HANDLE_VALUE {
                    return Err(last_error());
                }
                Ok(Self { handle, server: true })
            }
        }

        /// Wartet blockierend auf einen Client.
        pub fn accept(&self) -> io::Result<()> {
            unsafe {
                if ConnectNamedPipe(self.handle, ptr::null_mut()) == 0 && GetLastError() != ERROR_PIPE_CONNECTED {
                    return Err(last_error());
                }
            }
            Ok(())
        }

        /// Verbindet sich als Client. Der Server darf sich nicht als dieser Prozess ausgeben
        /// (`SECURITY_IDENTIFICATION`).
        pub fn connect() -> io::Result<Self> {
            let name = wide(&pipe_name()?);
            for attempt in 0..2 {
                let handle = unsafe {
                    CreateFileW(
                        name.as_ptr(),
                        GENERIC_READ | GENERIC_WRITE,
                        0,
                        ptr::null(),
                        OPEN_EXISTING,
                        SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                        ptr::null_mut(),
                    )
                };
                if handle != INVALID_HANDLE_VALUE {
                    return Ok(Self { handle, server: false });
                }
                let err = last_error();
                if attempt == 0 && err.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) {
                    unsafe { WaitNamedPipeW(name.as_ptr(), 2000) };
                    continue;
                }
                return Err(err);
            }
            Err(io::Error::new(io::ErrorKind::TimedOut, "Pipe ist belegt"))
        }

        /// Ist die Gegenstelle noch verbunden? Liest nichts aus der Pipe.
        pub fn is_alive(&self) -> bool {
            unsafe {
                PeekNamedPipe(self.handle, ptr::null_mut(), 0, ptr::null_mut(), ptr::null_mut(), ptr::null_mut()) != 0
            }
        }

        /// Programm am anderen Ende der Verbindung.
        pub fn peer_image(&self) -> io::Result<PathBuf> {
            let mut pid = 0u32;
            let ok = unsafe {
                if self.server {
                    GetNamedPipeClientProcessId(self.handle, &mut pid)
                } else {
                    GetNamedPipeServerProcessId(self.handle, &mut pid)
                }
            };
            if ok == 0 {
                return Err(last_error());
            }
            process_image(pid)
        }
    }

    /// Verbindet sich mit der App und prüft, dass sie aus demselben Verzeichnis stammt wie
    /// dieses Programm (Schutz vor einem fremden Prozess, der den Pipe-Namen belegt).
    pub fn connect_to_app() -> Result<Pipe, String> {
        let pipe = Pipe::connect().map_err(|_| "Remember Key läuft nicht. Bitte die App starten.".to_string())?;
        let ours = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let theirs = pipe.peer_image().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
        match (ours, theirs) {
            (Some(a), Some(b)) if crate::same_path(&a, &b) => Ok(pipe),
            _ => Err("Die Gegenstelle ist nicht Remember Key. Verbindung abgebrochen.".into()),
        }
    }

    impl Read for Pipe {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let mut n = 0u32;
            let len = buf.len().min(u32::MAX as usize) as u32;
            if unsafe { ReadFile(self.handle, buf.as_mut_ptr(), len, &mut n, ptr::null_mut()) } == 0 {
                return Err(last_error());
            }
            Ok(n as usize)
        }
    }

    impl Write for Pipe {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut n = 0u32;
            let len = buf.len().min(u32::MAX as usize) as u32;
            if unsafe { WriteFile(self.handle, buf.as_ptr(), len, &mut n, ptr::null_mut()) } == 0 {
                return Err(last_error());
            }
            Ok(n as usize)
        }

        fn flush(&mut self) -> io::Result<()> {
            if unsafe { FlushFileBuffers(self.handle) } == 0 {
                return Err(last_error());
            }
            Ok(())
        }
    }

    impl Drop for Pipe {
        fn drop(&mut self) {
            unsafe {
                if self.server {
                    DisconnectNamedPipe(self.handle);
                }
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_roundtrip() {
        let mut buf = Vec::new();
        write_message(&mut buf, &Request::ListEntries { query: Some("git".into()) }).unwrap();
        let req: Request = read_message(&mut buf.as_slice()).unwrap();
        assert!(matches!(req, Request::ListEntries { query: Some(q) } if q == "git"));
    }

    #[test]
    fn response_roundtrip() {
        let mut buf = Vec::new();
        let resp: Response = Err("gesperrt".into());
        write_message(&mut buf, &resp).unwrap();
        let back: Response = read_message(&mut buf.as_slice()).unwrap();
        assert_eq!(back, Err("gesperrt".into()));
    }

    #[test]
    fn rejects_oversized_length() {
        let mut buf = ((MAX_MESSAGE + 1) as u32).to_le_bytes().to_vec();
        buf.extend_from_slice(b"{}");
        assert!(read_message::<Response>(&mut buf.as_slice()).is_err());
    }

    #[test]
    fn http_request_defaults() {
        let req: Request =
            serde_json::from_str(r#"{"op":"http_request","entryId":"1","url":"https://x.de"}"#).unwrap();
        let Request::HttpRequest(r) = req else { panic!() };
        assert_eq!(r.method, "GET");
        assert!(r.headers.is_empty() && r.body.is_none() && r.auth.is_none());
        assert!(r.session.is_none() && !r.end_session);
        let req: Request = serde_json::from_str(
            r#"{"op":"http_request","entryId":"1","url":"https://x.de","session":"s_1","endSession":true}"#,
        )
        .unwrap();
        let Request::HttpRequest(r) = req else { panic!() };
        assert_eq!(r.session.as_deref(), Some("s_1"));
        assert!(r.end_session);
    }

    #[test]
    fn browser_messages() {
        let cmd = BrowserCommand::Fill {
            id: 7,
            hosts: vec!["x.de".into()],
            host_hint: None,
            username: "u".into(),
            password: "p".into(),
            otp: None,
        };
        let json = serde_json::to_value(&cmd).unwrap();
        assert_eq!(json["type"], "fill");
        assert_eq!(json["hostHint"], serde_json::Value::Null);
        let req: Request =
            serde_json::from_str(r#"{"op":"browser_result","id":7,"result":{"Err":"kein Tab"}}"#).unwrap();
        assert!(matches!(req, Request::BrowserResult { id: 7, result: Err(_) }));
        let req: Request = serde_json::from_str(r#"{"op":"fill_login","entryId":"1"}"#).unwrap();
        assert!(matches!(req, Request::FillLogin(FillLogin { url: None, .. })));
    }

    #[test]
    fn query_is_optional() {
        let req: Request = serde_json::from_str(r#"{"op":"list_entries"}"#).unwrap();
        assert!(matches!(req, Request::ListEntries { query: None }));
    }
}
