//! Verschlüsselte Tresor-Datei.
//!
//! Dateiformat (alle Zahlen little-endian):
//!
//! | Offset | Länge | Inhalt                         |
//! |--------|-------|--------------------------------|
//! | 0      | 4     | Magic `RKV1`                   |
//! | 4      | 1     | Formatversion (1)              |
//! | 5      | 4     | Argon2id m_cost (KiB)          |
//! | 9      | 4     | Argon2id t_cost                |
//! | 13     | 4     | Argon2id p_cost                |
//! | 17     | 16    | Salt                           |
//! | 33     | 24    | XChaCha20-Poly1305 Nonce       |
//! | 57     | …     | Ciphertext + 16 Byte Tag       |
//!
//! Der komplette Header (Byte 0–56) wird als AAD authentifiziert, sodass
//! Manipulationen an KDF-Parametern, Salt oder Nonce beim Entschlüsseln
//! auffallen. Bei jedem Speichern wird eine neue Nonce erzeugt.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::error::{Error, Result};

const MAGIC: &[u8; 4] = b"RKV1";
const FORMAT_VERSION: u8 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
const HEADER_LEN: usize = 4 + 1 + 12 + SALT_LEN + NONCE_LEN;

pub const MIN_MASTER_PASSWORD_LEN: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for KdfParams {
    /// 256 MiB, 3 Iterationen, 1 Lane – deutlich über der OWASP-Mindestempfehlung,
    /// weil die Datei per Cloud-Sync auch Dritten in die Hände fallen kann.
    fn default() -> Self {
        Self { m_cost: 256 * 1024, t_cost: 3, p_cost: 1 }
    }
}

impl KdfParams {
    /// Schwächer als der aktuelle Standard → beim nächsten Entsperren umstellen.
    pub fn is_weaker_than_default(&self) -> bool {
        let d = Self::default();
        self.m_cost < d.m_cost || self.t_cost < d.t_cost
    }

    /// Schutz vor manipulierten Dateien, die absurde Parameter erzwingen wollen.
    fn validate(&self) -> Result<()> {
        let ok = (8 * 1024..=4 * 1024 * 1024).contains(&self.m_cost)
            && (1..=20).contains(&self.t_cost)
            && (1..=16).contains(&self.p_cost);
        if ok { Ok(()) } else { Err(Error::Format) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct TotpConfig {
    /// Base32, Großbuchstaben, ohne Padding.
    pub secret: String,
    /// "SHA1", "SHA256" oder "SHA512".
    pub algorithm: String,
    pub digits: u32,
    pub period: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub username: String,
    pub password: String,
    /// API-Token für Programmzugriffe (z. B. KI-Zugriff). Ist er gesetzt, wird er bei
    /// `http_request` statt des Passworts eingesetzt.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_token: String,
    pub url: String,
    pub notes: String,
    #[serde(default)]
    pub totp: Option<TotpConfig>,
    /// `None` = oberste Ebene (kein Ordner).
    #[serde(default)]
    pub folder_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// Freigabe für KI-Assistenten (MCP). Fehlt bei älteren Tresoren und nie genutzten Einträgen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentPolicy>,
}

/// Regeln, nach denen ein KI-Assistent einen Eintrag nutzen darf, ohne ihn zu sehen.
/// Enthält keine Geheimnisse; `Zeroize` nur, weil `Entry` beim Drop alle Felder nullt.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPolicy {
    pub enabled: bool,
    /// Normalisierte Hosts (`example.com`, `*.example.com`), an die die Zugangsdaten gebunden sind.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// Stellen, an denen das Passwort in HTTP-Anfragen eingesetzt werden darf. Leer = gar nicht.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[zeroize(skip)]
    pub auth: Vec<AuthLocation>,
    /// 0 = jede Nutzung einzeln bestätigen; sonst darf eine Freigabe so viele Minuten gelten.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub session_minutes: u32,
    /// Login darf von der Browser-Erweiterung ausgefüllt werden (`fill_login`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub fill_login: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Höchstdauer einer Sitzungsfreigabe in Minuten.
pub const MAX_SESSION_MINUTES: u32 = 60;

impl AgentPolicy {
    /// Ist `host` (aus einer geparsten URL, also bereits normalisiert) erlaubt?
    /// `*.example.com` deckt nur Subdomains ab, nicht `example.com` selbst.
    pub fn allows_host(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        self.hosts.iter().any(|h| match h.strip_prefix("*.") {
            Some(base) => host.strip_suffix(base).is_some_and(|sub| sub.len() > 1 && sub.ends_with('.')),
            None => *h == host,
        })
    }
}

/// Wo das Passwort in eine HTTP-Anfrage eingesetzt wird.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AuthLocation {
    /// `Authorization: Bearer <Passwort>`
    Bearer,
    /// `Authorization: Basic base64(<Benutzername>:<Passwort>)`
    Basic,
    /// `<name>: <Passwort>`
    Header { name: String },
    /// Feld `<name>` in einem Formular-Body (`application/x-www-form-urlencoded`).
    FormField { name: String },
    /// Feld `<name>` auf oberster Ebene eines JSON-Objekts im Body.
    JsonField { name: String },
}

/// Header, die weder als Einsetz-Stelle noch von der KI gesetzt werden dürfen.
pub const FORBIDDEN_HEADERS: [&str; 7] =
    ["host", "cookie", "content-length", "transfer-encoding", "connection", "proxy-authorization", "te"];

impl AuthLocation {
    /// Kurzform für KI und Protokoll: `bearer`, `basic`, `header:X-Api-Key`.
    pub fn key(&self) -> String {
        match self {
            Self::Bearer => "bearer".into(),
            Self::Basic => "basic".into(),
            Self::Header { name } => format!("header:{name}"),
            Self::FormField { name } => format!("form:{name}"),
            Self::JsonField { name } => format!("json:{name}"),
        }
    }

    /// Name des Headers, in dem das Geheimnis landet; `None` bei Feldern im Body.
    pub fn header_name(&self) -> Option<&str> {
        match self {
            Self::Bearer | Self::Basic => Some("Authorization"),
            Self::Header { name } => Some(name),
            Self::FormField { .. } | Self::JsonField { .. } => None,
        }
    }

    /// Name des Body-Felds, in dem das Geheimnis landet; `None` bei Headern.
    pub fn body_field(&self) -> Option<&str> {
        match self {
            Self::FormField { name } | Self::JsonField { name } => Some(name),
            _ => None,
        }
    }

    /// Prüft und normalisiert (Namen getrimmt; Header nur aus Token-Zeichen).
    pub fn validated(&self) -> Result<Self> {
        match self {
            Self::Bearer | Self::Basic => Ok(self.clone()),
            Self::Header { name } => {
                let name = name.trim();
                let token = !name.is_empty()
                    && name.len() <= 64
                    && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
                if !token || FORBIDDEN_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
                    return Err(Error::Invalid(format!("Ungültiger Header-Name: {name}")));
                }
                Ok(Self::Header { name: name.to_string() })
            }
            Self::FormField { name } | Self::JsonField { name } => {
                let trimmed = name.trim();
                if trimmed.is_empty() || trimmed.chars().count() > 64 || trimmed.chars().any(char::is_control) {
                    return Err(Error::Invalid(format!("Ungültiger Feldname: {trimmed}")));
                }
                let name = trimmed.to_string();
                Ok(match self {
                    Self::FormField { .. } => Self::FormField { name },
                    _ => Self::JsonField { name },
                })
            }
        }
    }
}

/// Normalisiert eine Host-Angabe: URL oder Host, optional mit `*.`-Präfix für Subdomains.
/// Internationale Domains werden in Punycode umgewandelt.
pub fn normalize_host(input: &str) -> Result<String> {
    let s = input.trim();
    let invalid = || Error::Invalid(format!("Ungültiger Host: {s}"));
    if s.is_empty() {
        return Err(invalid());
    }
    let (wildcard, rest) = match s.strip_prefix("*.") {
        Some(r) => (true, r),
        None => (false, s),
    };
    let host = if rest.contains("://") {
        url::Url::parse(rest).map_err(|_| invalid())?.host_str().ok_or_else(invalid)?.to_string()
    } else {
        if rest.contains(['/', ':', '@', '?', '#', '*']) {
            return Err(invalid());
        }
        match url::Host::parse(rest).map_err(|_| invalid())? {
            url::Host::Domain(d) => d,
            other => other.to_string(),
        }
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || (wildcard && !host.contains('.')) {
        return Err(invalid());
    }
    Ok(if wildcard { format!("*.{host}") } else { host })
}

#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub name: String,
    /// `None` = Ordner liegt auf oberster Ebene.
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub updated_at: i64,
}

/// Löschvermerk, damit ein Löschen beim Zusammenführen nicht von einem anderen
/// Gerät rückgängig gemacht wird. Gilt für Einträge und Ordner (UUIDs sind eindeutig).
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct Tombstone {
    pub id: String,
    pub deleted_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Zeroize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Automatische Sperre nach Inaktivität in Minuten.
    pub auto_lock_minutes: u32,
    /// Zwischenablage wird nach dieser Zeit geleert.
    pub clipboard_clear_seconds: u32,
    /// Zeitpunkt der letzten Änderung (für die Zusammenführung zwischen Geräten).
    #[serde(default)]
    pub updated_at: i64,
    /// KI-Assistenten dürfen über die MCP-Brücke auf freigegebene Einträge zugreifen.
    #[serde(default)]
    pub agent_enabled: bool,
    /// Freigaben von KI-Anfragen zusätzlich mit Windows Hello bestätigen.
    #[serde(default)]
    pub agent_hello: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { auto_lock_minutes: 5, clipboard_clear_seconds: 30, updated_at: 0, agent_enabled: false, agent_hello: false }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !(1..=240).contains(&self.auto_lock_minutes) {
            return Err(Error::Invalid("Auto-Sperre muss zwischen 1 und 240 Minuten liegen".into()));
        }
        if !(5..=300).contains(&self.clipboard_clear_seconds) {
            return Err(Error::Invalid(
                "Zwischenablage-Timeout muss zwischen 5 und 300 Sekunden liegen".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct VaultData {
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(default)]
    pub deleted: Vec<Tombstone>,
    #[serde(default)]
    pub settings: Settings,
    /// Zeitpunkt des letzten Schlüsselwechsels (Passwortwechsel, KDF-Umstellung).
    /// Beim Abgleich entscheidet er, welcher Schlüssel gilt.
    #[serde(default)]
    pub key_changed_at: i64,
}

const MAX_FOLDER_DEPTH: usize = 32;

impl VaultData {
    pub fn folder_exists(&self, id: &str) -> bool {
        self.folders.iter().any(|f| f.id == id)
    }

    fn parent_of(&self, id: &str) -> Option<&str> {
        self.folders.iter().find(|f| f.id == id)?.parent_id.as_deref()
    }

    /// Prüft einen Ziel-Elternordner: muss existieren und darf weder der Ordner
    /// selbst noch einer seiner Unterordner sein (sonst entstünde ein Zyklus).
    fn check_parent(&self, folder_id: Option<&str>, parent: Option<&str>) -> Result<()> {
        let Some(parent) = parent else { return Ok(()) };
        if !self.folder_exists(parent) {
            return Err(Error::Invalid("Übergeordneter Ordner existiert nicht".into()));
        }
        let mut cur = Some(parent);
        let mut depth = 0;
        while let Some(c) = cur {
            if Some(c) == folder_id {
                return Err(Error::Invalid(
                    "Ein Ordner kann nicht in sich selbst oder einen Unterordner verschoben werden".into(),
                ));
            }
            depth += 1;
            if depth > MAX_FOLDER_DEPTH {
                return Err(Error::Invalid("Ordnerstruktur ist zu tief verschachtelt".into()));
            }
            cur = self.parent_of(c);
        }
        Ok(())
    }

    fn validate_folder_name(name: &str) -> Result<String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(Error::Invalid("Ordnername muss 1–80 Zeichen lang sein".into()));
        }
        Ok(name.to_string())
    }

    pub fn create_folder(&mut self, id: String, name: &str, parent: Option<String>, ts: i64) -> Result<()> {
        let name = Self::validate_folder_name(name)?;
        self.check_parent(None, parent.as_deref())?;
        self.folders.push(Folder { id, name, parent_id: parent, updated_at: ts });
        Ok(())
    }

    pub fn update_folder(&mut self, id: &str, name: &str, parent: Option<String>, ts: i64) -> Result<()> {
        let name = Self::validate_folder_name(name)?;
        self.check_parent(Some(id), parent.as_deref())?;
        let f = self.folders.iter_mut().find(|f| f.id == id).ok_or(Error::NotFound)?;
        f.name = name;
        f.parent_id = parent;
        f.updated_at = ts;
        Ok(())
    }

    fn tombstone(&mut self, id: &str, ts: i64) {
        match self.deleted.iter_mut().find(|t| t.id == id) {
            Some(t) => t.deleted_at = t.deleted_at.max(ts),
            None => self.deleted.push(Tombstone { id: id.to_string(), deleted_at: ts }),
        }
    }

    /// Löscht einen Ordner. Unterordner und Einträge wandern eine Ebene nach oben.
    pub fn delete_folder(&mut self, id: &str, ts: i64) -> Result<()> {
        let pos = self.folders.iter().position(|f| f.id == id).ok_or(Error::NotFound)?;
        let parent = self.folders[pos].parent_id.clone();
        self.folders.remove(pos);
        for f in self.folders.iter_mut().filter(|f| f.parent_id.as_deref() == Some(id)) {
            f.parent_id = parent.clone();
            f.updated_at = ts;
        }
        for e in self.entries.iter_mut().filter(|e| e.folder_id.as_deref() == Some(id)) {
            e.folder_id = parent.clone();
            e.updated_at = ts;
        }
        self.tombstone(id, ts);
        Ok(())
    }

    pub fn delete_entry(&mut self, id: &str, ts: i64) -> Result<()> {
        let pos = self.entries.iter().position(|e| e.id == id).ok_or(Error::NotFound)?;
        self.entries.remove(pos);
        self.tombstone(id, ts);
        Ok(())
    }

    /// Führt den Stand eines anderen Geräts ein. Pro Eintrag/Ordner gewinnt die
    /// jüngere Änderung; ein Löschvermerk gewinnt gegen ältere Änderungen.
    /// Das Ergebnis ist unabhängig von der Reihenfolge der Geräte.
    pub fn merge(&mut self, other: &VaultData) {
        for t in &other.deleted {
            self.tombstone(&t.id, t.deleted_at);
        }
        merge_items(&mut self.entries, &other.entries, |e| (&e.id, e.updated_at));
        merge_items(&mut self.folders, &other.folders, |f| (&f.id, f.updated_at));

        let dead: HashMap<String, i64> = self.deleted.iter().map(|t| (t.id.clone(), t.deleted_at)).collect();
        let alive = |id: &str, updated: i64| dead.get(id).is_none_or(|&d| updated > d);
        self.entries.retain(|e| alive(&e.id, e.updated_at));
        self.folders.retain(|f| alive(&f.id, f.updated_at));

        self.key_changed_at = self.key_changed_at.max(other.key_changed_at);
        let s = other.settings;
        let key =
            |s: &Settings| (s.updated_at, s.auto_lock_minutes, s.clipboard_clear_seconds, s.agent_enabled, s.agent_hello);
        if key(&s) > key(&self.settings) {
            self.settings = s;
        }
        self.repair();
    }

    /// Behebt Widersprüche, die nur durch Zusammenführen entstehen können:
    /// verwaiste Verweise und Ordnerzyklen (A in B auf Gerät 1, B in A auf Gerät 2).
    fn repair(&mut self) {
        let ids: HashSet<String> = self.folders.iter().map(|f| f.id.clone()).collect();
        for f in self.folders.iter_mut() {
            if f.parent_id.as_ref().is_some_and(|p| !ids.contains(p)) {
                f.parent_id = None;
            }
        }
        for e in self.entries.iter_mut() {
            if e.folder_id.as_ref().is_some_and(|p| !ids.contains(p)) {
                e.folder_id = None;
            }
        }
        // Zyklen deterministisch auflösen: der Ordner mit der kleinsten ID im Zyklus wandert nach oben.
        loop {
            let parents: HashMap<String, Option<String>> =
                self.folders.iter().map(|f| (f.id.clone(), f.parent_id.clone())).collect();
            let mut cycle_min: Option<String> = None;
            for f in &self.folders {
                let mut seen = vec![f.id.clone()];
                let mut cur = parents.get(&f.id).cloned().flatten();
                while let Some(c) = cur {
                    if let Some(pos) = seen.iter().position(|s| *s == c) {
                        cycle_min = seen[pos..].iter().min().cloned();
                        break;
                    }
                    seen.push(c.clone());
                    cur = parents.get(&c).cloned().flatten();
                }
                if cycle_min.is_some() {
                    break;
                }
            }
            match cycle_min {
                Some(id) => {
                    if let Some(f) = self.folders.iter_mut().find(|f| f.id == id) {
                        f.parent_id = None;
                    }
                }
                None => break,
            }
        }
    }

    pub fn check_entry_folder(&self, folder: Option<&str>) -> Result<()> {
        match folder {
            Some(f) if !self.folder_exists(f) => Err(Error::Invalid("Ordner existiert nicht".into())),
            _ => Ok(()),
        }
    }
}

/// Übernimmt Elemente aus `other`, wenn sie neu oder jünger sind. Bei gleichem
/// Zeitstempel entscheidet der serialisierte Inhalt, damit alle Geräte gleich wählen.
fn merge_items<T: Clone + Serialize>(mine: &mut Vec<T>, other: &[T], key: impl Fn(&T) -> (&String, i64)) {
    for o in other {
        let (oid, ots) = key(o);
        match mine.iter_mut().find(|m| key(m).0 == oid) {
            Some(m) => {
                let mts = key(m).1;
                let newer = ots > mts
                    || (ots == mts
                        && serde_json::to_string(o).unwrap_or_default() > serde_json::to_string(m).unwrap_or_default());
                if newer {
                    *m = o.clone();
                }
            }
            None => mine.push(o.clone()),
        }
    }
}

/// Ergebnis beim Öffnen eines fremden Tresorstands (z. B. aus Google Drive).
pub enum RemoteOpen {
    Data(VaultData),
    /// Anderer Salt/KDF – das Master-Passwort wird benötigt (z. B. nach Passwortwechsel).
    NeedsPassword,
}

struct ParsedFile<'a> {
    kdf: KdfParams,
    salt: [u8; SALT_LEN],
    header: &'a [u8],
    ciphertext: &'a [u8],
}

fn parse_file(bytes: &[u8]) -> Result<ParsedFile<'_>> {
    if bytes.len() < HEADER_LEN + 16 || &bytes[0..4] != MAGIC || bytes[4] != FORMAT_VERSION {
        return Err(Error::Format);
    }
    let kdf = KdfParams {
        m_cost: read_u32(bytes, 5),
        t_cost: read_u32(bytes, 9),
        p_cost: read_u32(bytes, 13),
    };
    kdf.validate()?;
    let salt: [u8; SALT_LEN] = bytes[17..33].try_into().unwrap();
    let (header, ciphertext) = bytes.split_at(HEADER_LEN);
    Ok(ParsedFile { kdf, salt, header, ciphertext })
}

fn decrypt_with(key: &SecretKey, file: &ParsedFile) -> Result<VaultData> {
    let cipher = XChaCha20Poly1305::new(key.bytes().into());
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(XNonce::from_slice(&file.header[33..57]), Payload { msg: file.ciphertext, aad: file.header })
            .map_err(|_| Error::Decrypt)?,
    );
    serde_json::from_slice(&plaintext).map_err(|_| Error::Format)
}

/// Prüft nur, ob die Bytes formal eine Tresor-Datei sind (ohne Entschlüsselung).
pub fn looks_like_vault(bytes: &[u8]) -> bool {
    parse_file(bytes).is_ok()
}

/// 32-Byte-Schlüssel auf dem Heap. Unter Windows per `VirtualLock` vom Auslagern
/// in die Auslagerungsdatei ausgenommen (nicht vom Ruhezustand), beim Drop genullt.
pub struct SecretKey(Box<[u8; KEY_LEN]>);

impl SecretKey {
    fn new() -> Self {
        let k = SecretKey(Box::new([0u8; KEY_LEN]));
        lock_memory(k.0.as_ptr(), KEY_LEN);
        k
    }

    fn bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl Clone for SecretKey {
    fn clone(&self) -> Self {
        let mut k = Self::new();
        k.0.copy_from_slice(&self.0[..]);
        k
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.zeroize();
        // Kein VirtualUnlock: Sperren sind seitenweise, nicht gezählt – ein Unlock
        // könnte einen anderen Schlüssel auf derselben Seite mit entsperren.
    }
}

#[cfg(windows)]
fn lock_memory(ptr: *const u8, len: usize) {
    unsafe {
        windows_sys::Win32::System::Memory::VirtualLock(ptr as *const core::ffi::c_void, len);
    }
}

#[cfg(not(windows))]
fn lock_memory(_ptr: *const u8, _len: usize) {}

/// Serialisiert in einen Puffer exakter Größe, damit beim Wachsen keine
/// ungenullten Kopien des Klartexts im Speicher zurückbleiben.
pub fn to_json_zeroizing<T: Serialize>(value: &T) -> Result<Zeroizing<Vec<u8>>> {
    struct Counter(usize);
    impl Write for Counter {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0 += b.len();
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value)?;
    let mut buf = Zeroizing::new(Vec::with_capacity(counter.0));
    serde_json::to_writer(&mut *buf, value)?;
    Ok(buf)
}

struct PreviousKey {
    key: SecretKey,
    kdf: KdfParams,
    salt: [u8; SALT_LEN],
}

/// Entsperrter Tresor im Speicher. Schlüssel und Inhalte werden beim Drop genullt.
pub struct UnlockedVault {
    key: SecretKey,
    kdf: KdfParams,
    salt: [u8; SALT_LEN],
    /// Schlüssel vor dem letzten Wechsel – nur so lange, bis die Kopie in Google
    /// Drive mit dem neuen Schlüssel ersetzt wurde.
    previous: Option<PreviousKey>,
    pub data: VaultData,
}

fn derive_key(password: &[u8], salt: &[u8], kdf: KdfParams) -> Result<SecretKey> {
    let params = Params::new(kdf.m_cost, kdf.t_cost, kdf.p_cost, Some(KEY_LEN))
        .map_err(|_| Error::Format)?;
    let mut key = SecretKey::new();
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password, salt, &mut key.0[..])
        .map_err(|_| Error::Format)?;
    Ok(key)
}

fn build_header(kdf: KdfParams, salt: &[u8; SALT_LEN], nonce: &[u8; NONCE_LEN]) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[0..4].copy_from_slice(MAGIC);
    h[4] = FORMAT_VERSION;
    h[5..9].copy_from_slice(&kdf.m_cost.to_le_bytes());
    h[9..13].copy_from_slice(&kdf.t_cost.to_le_bytes());
    h[13..17].copy_from_slice(&kdf.p_cost.to_le_bytes());
    h[17..33].copy_from_slice(salt);
    h[33..57].copy_from_slice(nonce);
    h
}

fn read_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

impl UnlockedVault {
    pub fn create(password: &str) -> Result<Self> {
        Self::create_with(password, KdfParams::default())
    }

    fn create_with(password: &str, kdf: KdfParams) -> Result<Self> {
        if password.chars().count() < MIN_MASTER_PASSWORD_LEN {
            return Err(Error::Invalid(format!(
                "Das Master-Passwort muss mindestens {MIN_MASTER_PASSWORD_LEN} Zeichen lang sein"
            )));
        }
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let key = derive_key(password.as_bytes(), &salt, kdf)?;
        Ok(Self { key, kdf, salt, previous: None, data: VaultData::default() })
    }

    pub fn open(bytes: &[u8], password: &str) -> Result<Self> {
        let file = parse_file(bytes)?;
        let key = derive_key(password.as_bytes(), &file.salt, file.kdf)?;
        let data = decrypt_with(&key, &file)?;
        Ok(Self { key, kdf: file.kdf, salt: file.salt, previous: None, data })
    }

    /// Entschlüsselt einen fremden Stand mit dem vorhandenen Schlüssel, sofern
    /// Salt und KDF-Parameter übereinstimmen (ohne erneute Schlüsselableitung).
    pub fn open_remote(&self, bytes: &[u8]) -> Result<RemoteOpen> {
        let file = parse_file(bytes)?;
        if file.salt == self.salt && file.kdf == self.kdf {
            return Ok(RemoteOpen::Data(decrypt_with(&self.key, &file)?));
        }
        if let Some(p) = self.previous.as_ref().filter(|p| p.salt == file.salt && p.kdf == file.kdf) {
            return Ok(RemoteOpen::Data(decrypt_with(&p.key, &file)?));
        }
        Ok(RemoteOpen::NeedsPassword)
    }

    /// Wurde `bytes` mit dem aktuellen Schlüssel gespeichert?
    pub fn is_current_key_file(&self, bytes: &[u8]) -> bool {
        parse_file(bytes).is_ok_and(|f| f.salt == self.salt && f.kdf == self.kdf)
    }

    /// Ein Schlüsselwechsel ist lokal erfolgt, die Kopie in Google Drive aber noch alt.
    pub fn rekey_pending(&self) -> bool {
        self.previous.is_some()
    }

    pub fn finish_rekey(&mut self) {
        self.previous = None;
    }

    pub fn needs_kdf_upgrade(&self) -> bool {
        self.kdf.is_weaker_than_default()
    }

    /// Neuer Salt, aktuelle KDF-Parameter. Der bisherige Schlüssel wird gemerkt,
    /// bis die Kopie in Google Drive ersetzt ist (nur der älteste, falls mehrfach).
    fn rekey(&mut self, password: &str, ts: i64) -> Result<()> {
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let kdf = KdfParams::default();
        let key = derive_key(password.as_bytes(), &salt, kdf)?;
        if self.previous.is_none() {
            self.previous = Some(PreviousKey { key: self.key.clone(), kdf: self.kdf, salt: self.salt });
        }
        self.key = key;
        self.kdf = kdf;
        self.salt = salt;
        self.data.key_changed_at = ts;
        Ok(())
    }

    /// Stellt auf die aktuellen Argon2-Parameter um (Passwort bleibt gleich).
    pub fn upgrade_kdf(&mut self, password: &str, ts: i64) -> Result<()> {
        self.rekey(password, ts)
    }

    /// Übernimmt Schlüssel, Salt und KDF eines anderen Stands, damit beide Geräte
    /// danach denselben Schlüssel verwenden. Die Daten bleiben unverändert.
    pub fn adopt_key(&mut self, other: &UnlockedVault) {
        self.key = other.key.clone();
        self.kdf = other.kdf;
        self.salt = other.salt;
        // Ein lokal noch ausstehender Schlüsselwechsel ist damit überholt.
        self.previous = None;
    }

    /// Verschlüsselt den aktuellen Inhalt mit frischer Nonce.
    pub fn seal(&self) -> Result<Vec<u8>> {
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let header = build_header(self.kdf, &self.salt, &nonce);
        let plaintext = to_json_zeroizing(&self.data)?;
        let cipher = XChaCha20Poly1305::new(self.key.bytes().into());
        let ciphertext = cipher
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: &plaintext, aad: &header })
            .map_err(|_| Error::Format)?;
        let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
        out.extend_from_slice(&header);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Prüft ein Passwort gegen den aktuellen Schlüssel (für Passwortwechsel).
    pub fn verify_password(&self, password: &str) -> Result<bool> {
        let key = derive_key(password.as_bytes(), &self.salt, self.kdf)?;
        Ok(constant_time_eq(key.bytes(), self.key.bytes()))
    }

    pub fn change_password(&mut self, new_password: &str, ts: i64) -> Result<()> {
        if new_password.chars().count() < MIN_MASTER_PASSWORD_LEN {
            return Err(Error::Invalid(format!(
                "Das Master-Passwort muss mindestens {MIN_MASTER_PASSWORD_LEN} Zeichen lang sein"
            )));
        }
        self.rekey(new_password, ts)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Schreibt atomar: temporäre Datei → fsync → Backup der alten Version → Umbenennen.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("rk.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if path.exists() {
        fs::copy(path, path.with_extension("rk.bak"))?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Schnelle Parameter für Tests.
    const TEST_KDF: KdfParams = KdfParams { m_cost: 8 * 1024, t_cost: 1, p_cost: 1 };

    fn sample_entry() -> Entry {
        Entry {
            id: "1".into(),
            title: "Mail".into(),
            username: "me@example.com".into(),
            password: "s3cr3t!".into(),
            api_token: String::new(),
            url: "https://mail.example.com".into(),
            notes: String::new(),
            totp: None,
            folder_id: None,
            created_at: 0,
            updated_at: 0,
            agent: None,
        }
    }

    #[test]
    fn folders_delete_reparents_children() {
        let mut d = VaultData::default();
        d.create_folder("a".into(), "Arbeit", None, 1).unwrap();
        d.create_folder("b".into(), "Kunden", Some("a".into()), 1).unwrap();
        d.create_folder("c".into(), "Kunde X", Some("b".into()), 1).unwrap();
        let mut e = sample_entry();
        e.folder_id = Some("b".into());
        d.entries.push(e);

        d.delete_folder("b", 2).unwrap();
        assert!(!d.folder_exists("b"));
        assert_eq!(d.folders.iter().find(|f| f.id == "c").unwrap().parent_id.as_deref(), Some("a"));
        assert_eq!(d.entries[0].folder_id.as_deref(), Some("a"));

        d.delete_folder("a", 3).unwrap();
        assert_eq!(d.folders.iter().find(|f| f.id == "c").unwrap().parent_id, None);
        assert_eq!(d.entries[0].folder_id, None);
    }

    #[test]
    fn folders_reject_cycles_and_bad_input() {
        let mut d = VaultData::default();
        d.create_folder("a".into(), "A", None, 1).unwrap();
        d.create_folder("b".into(), "B", Some("a".into()), 1).unwrap();
        assert!(d.update_folder("a", "A", Some("b".into()), 2).is_err()); // Zyklus
        assert!(d.update_folder("a", "A", Some("a".into()), 2).is_err()); // sich selbst
        assert!(d.create_folder("x".into(), "X", Some("fehlt".into()), 1).is_err());
        assert!(d.create_folder("y".into(), "   ", None, 1).is_err());
        d.update_folder("b", "B neu", None, 2).unwrap();
        assert_eq!(d.folders[1].name, "B neu");
        assert!(d.check_entry_folder(Some("fehlt")).is_err());
        assert!(d.check_entry_folder(None).is_ok());
    }

    fn entry(id: &str, title: &str, ts: i64) -> Entry {
        let mut e = sample_entry();
        e.id = id.into();
        e.title = title.into();
        e.updated_at = ts;
        e
    }

    fn titles(d: &VaultData) -> Vec<String> {
        let mut t: Vec<String> = d.entries.iter().map(|e| e.title.clone()).collect();
        t.sort();
        t
    }

    #[test]
    fn merge_newer_edit_wins_and_union() {
        let mut a = VaultData::default();
        a.entries.push(entry("1", "alt", 10));
        a.entries.push(entry("2", "nur A", 10));
        let mut b = VaultData::default();
        b.entries.push(entry("1", "neu", 20));
        b.entries.push(entry("3", "nur B", 15));

        let mut ab = VaultData::default();
        ab.merge(&a);
        ab.merge(&b);
        let mut ba = VaultData::default();
        ba.merge(&b);
        ba.merge(&a);
        assert_eq!(titles(&ab), vec!["neu", "nur A", "nur B"]);
        assert_eq!(titles(&ab), titles(&ba));
    }

    #[test]
    fn merge_delete_beats_older_edit_but_not_newer() {
        let mut a = VaultData::default();
        a.entries.push(entry("1", "x", 10));
        a.entries.push(entry("2", "y", 10));
        a.delete_entry("1", 20).unwrap();
        a.delete_entry("2", 20).unwrap();

        let mut b = VaultData::default();
        b.entries.push(entry("1", "x geändert vorher", 15));
        b.entries.push(entry("2", "y geändert nachher", 25));

        b.merge(&a);
        assert_eq!(titles(&b), vec!["y geändert nachher"]);
        // Erneutes Zusammenführen ändert nichts mehr (idempotent).
        b.merge(&a);
        assert_eq!(titles(&b), vec!["y geändert nachher"]);
    }

    #[test]
    fn merge_entry_in_deleted_folder_moves_up() {
        let mut a = VaultData::default();
        a.create_folder("f".into(), "Ordner", None, 1).unwrap();
        let mut b = VaultData::default();
        b.merge(&a);

        a.delete_folder("f", 5).unwrap();
        let mut e = entry("1", "neu im Ordner", 6);
        e.folder_id = Some("f".into());
        b.entries.push(e);

        b.merge(&a);
        assert!(b.folders.is_empty());
        assert_eq!(b.entries[0].folder_id, None);
    }

    #[test]
    fn merge_resolves_folder_cycle() {
        let mut base = VaultData::default();
        base.create_folder("a".into(), "A", None, 1).unwrap();
        base.create_folder("b".into(), "B", None, 1).unwrap();
        let mut d1 = VaultData::default();
        d1.merge(&base);
        let mut d2 = VaultData::default();
        d2.merge(&base);
        d1.update_folder("a", "A", Some("b".into()), 5).unwrap();
        d2.update_folder("b", "B", Some("a".into()), 6).unwrap();

        d1.merge(&d2);
        d2.merge(&d1);
        let parent = |d: &VaultData, id: &str| d.folders.iter().find(|f| f.id == id).unwrap().parent_id.clone();
        assert_eq!(parent(&d1, "a"), None); // kleinste ID im Zyklus wandert nach oben
        assert_eq!(parent(&d1, "b"), Some("a".into()));
        assert_eq!(parent(&d1, "a"), parent(&d2, "a"));
        assert_eq!(parent(&d1, "b"), parent(&d2, "b"));
    }

    #[test]
    fn merge_settings_newer_wins() {
        let mut a = VaultData::default();
        let mut b = VaultData::default();
        b.settings = Settings { auto_lock_minutes: 15, clipboard_clear_seconds: 30, updated_at: 9, agent_enabled: true, agent_hello: false };
        a.merge(&b);
        assert_eq!(a.settings.auto_lock_minutes, 15);
        assert!(a.settings.agent_enabled);
    }

    #[test]
    fn normalize_host_variants() {
        assert_eq!(normalize_host("https://GitHub.com/login").unwrap(), "github.com");
        assert_eq!(normalize_host(" api.github.com ").unwrap(), "api.github.com");
        assert_eq!(normalize_host("*.Example.com").unwrap(), "*.example.com");
        assert_eq!(normalize_host("bücher.de").unwrap(), "xn--bcher-kva.de");
        assert_eq!(normalize_host("example.com.").unwrap(), "example.com");
        for bad in ["", "*", "*.com", "example.com/pfad", "user@example.com", "example.com:8080", "a*.b.com"] {
            assert!(normalize_host(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn allows_host_exact_and_wildcard() {
        let p = AgentPolicy { enabled: true, hosts: vec!["github.com".into(), "*.azure.com".into()], ..Default::default() };
        assert!(p.allows_host("github.com"));
        assert!(p.allows_host("GitHub.com."));
        assert!(!p.allows_host("api.github.com"));
        assert!(!p.allows_host("evilgithub.com"));
        assert!(p.allows_host("management.azure.com"));
        assert!(p.allows_host("a.b.azure.com"));
        assert!(!p.allows_host("azure.com"));
        assert!(!p.allows_host("evilazure.com"));
        assert!(!p.allows_host(".azure.com"));
    }

    #[test]
    fn auth_location_validation_and_serde() {
        assert!(AuthLocation::Header { name: "X-Api-Key".into() }.validated().is_ok());
        assert_eq!(
            AuthLocation::Header { name: " X-Key ".into() }.validated().unwrap(),
            AuthLocation::Header { name: "X-Key".into() }
        );
        for bad in ["", "X Key", "Cookie", "host", "X:Key", "Ä"] {
            assert!(AuthLocation::Header { name: bad.into() }.validated().is_err(), "{bad}");
        }
        assert_eq!(
            AuthLocation::JsonField { name: " password ".into() }.validated().unwrap(),
            AuthLocation::JsonField { name: "password".into() }
        );
        assert!(AuthLocation::FormField { name: "".into() }.validated().is_err());
        assert!(AuthLocation::FormField { name: "a\nb".into() }.validated().is_err());
        assert_eq!(AuthLocation::FormField { name: "pw".into() }.key(), "form:pw");
        let json = serde_json::to_string(&vec![AuthLocation::Bearer, AuthLocation::Header { name: "X".into() }]).unwrap();
        assert_eq!(json, r#"[{"kind":"bearer"},{"kind":"header","name":"X"}]"#);
        // Phase-1-Freigaben ohne neue Felder laden weiter.
        let p: AgentPolicy = serde_json::from_str(r#"{"enabled":true,"hosts":["x.de"]}"#).unwrap();
        assert!(p.auth.is_empty() && p.session_minutes == 0);
    }

    #[test]
    fn agent_policy_is_optional_and_merges_with_entry() {
        // Einträge ohne Freigabe serialisieren unverändert (wichtig für den Gleichstand beim Merge).
        assert!(!serde_json::to_string(&sample_entry()).unwrap().contains("agent"));
        let mut a = VaultData::default();
        a.entries.push(entry("1", "Mail", 1));
        let mut b = VaultData::default();
        let mut e = entry("1", "Mail", 2);
        e.agent = Some(AgentPolicy { enabled: true, hosts: vec!["mail.example.com".into()], ..Default::default() });
        b.entries.push(e);
        a.merge(&b);
        assert_eq!(a.entries[0].agent.as_ref().unwrap().hosts, ["mail.example.com"]);
    }

    #[test]
    fn open_remote_same_key_and_adopt() {
        let local = UnlockedVault::create_with("correct horse battery", TEST_KDF).unwrap();
        let mut remote = UnlockedVault::open(&local.seal().unwrap(), "correct horse battery").unwrap();
        remote.data.entries.push(sample_entry());
        assert!(matches!(local.open_remote(&remote.seal().unwrap()).unwrap(), RemoteOpen::Data(d) if d.entries.len() == 1));

        // Anderer Salt (z. B. Passwortwechsel auf anderem Gerät) → Passwort nötig.
        let other = UnlockedVault::create_with("correct horse battery", TEST_KDF).unwrap();
        assert!(matches!(local.open_remote(&other.seal().unwrap()).unwrap(), RemoteOpen::NeedsPassword));
        let mut adopted = UnlockedVault::create_with("correct horse battery", TEST_KDF).unwrap();
        adopted.adopt_key(&other);
        assert!(matches!(adopted.open_remote(&other.seal().unwrap()).unwrap(), RemoteOpen::Data(_)));
        assert!(looks_like_vault(&other.seal().unwrap()));
        assert!(!looks_like_vault(b"nope"));
    }

    #[test]
    fn previous_key_opens_old_remote_until_rekey_finished() {
        let mut v = UnlockedVault::create_with("correct horse battery", TEST_KDF).unwrap();
        let old_remote = v.seal().unwrap();
        v.change_password("another long password", 5).unwrap();
        assert_eq!(v.data.key_changed_at, 5);
        // Alter Stand (alter Schlüssel) lässt sich ohne Passwort zusammenführen.
        assert!(matches!(v.open_remote(&old_remote).unwrap(), RemoteOpen::Data(_)));
        v.finish_rekey();
        assert!(matches!(v.open_remote(&old_remote).unwrap(), RemoteOpen::NeedsPassword));
        // Zweiter Wechsel vor dem Abgleich: der älteste Schlüssel bleibt gemerkt.
        let mut w = UnlockedVault::create_with("correct horse battery", TEST_KDF).unwrap();
        let w_old = w.seal().unwrap();
        w.change_password("second password!!", 6).unwrap();
        w.change_password("third password!!!", 7).unwrap();
        assert!(matches!(w.open_remote(&w_old).unwrap(), RemoteOpen::Data(_)));
    }

    #[test]
    fn kdf_upgrade_detected_and_applied() {
        let mut v = UnlockedVault::create_with("correct horse battery", TEST_KDF).unwrap();
        assert!(v.needs_kdf_upgrade());
        v.upgrade_kdf("correct horse battery", 9).unwrap();
        assert!(!v.needs_kdf_upgrade());
        let bytes = v.seal().unwrap();
        let reopened = UnlockedVault::open(&bytes, "correct horse battery").unwrap();
        assert!(!reopened.needs_kdf_upgrade());
        assert_eq!(reopened.data.key_changed_at, 9);
    }

    #[test]
    fn json_buffer_has_exact_capacity() {
        let mut d = VaultData::default();
        d.entries.push(sample_entry());
        let buf = to_json_zeroizing(&d).unwrap();
        assert_eq!(buf.len(), buf.capacity());
        assert_eq!(&buf[..], &serde_json::to_vec(&d).unwrap()[..]);
    }

    #[test]
    fn old_vault_without_folders_still_loads() {
        let json = r#"{"entries":[{"id":"1","title":"t","username":"","password":"","url":"","notes":"","createdAt":0,"updatedAt":0}]}"#;
        let d: VaultData = serde_json::from_str(json).unwrap();
        assert!(d.folders.is_empty());
        assert_eq!(d.entries[0].folder_id, None);
    }

    #[test]
    fn roundtrip() {
        let mut v = UnlockedVault::create_with("correct horse battery", TEST_KDF).unwrap();
        v.data.entries.push(sample_entry());
        let bytes = v.seal().unwrap();
        let v2 = UnlockedVault::open(&bytes, "correct horse battery").unwrap();
        assert_eq!(v2.data.entries.len(), 1);
        assert_eq!(v2.data.entries[0].password, "s3cr3t!");
    }

    #[test]
    fn wrong_password_fails() {
        let v = UnlockedVault::create_with("correct horse battery", TEST_KDF).unwrap();
        let bytes = v.seal().unwrap();
        assert!(matches!(UnlockedVault::open(&bytes, "wrong password!"), Err(Error::Decrypt)));
    }

    #[test]
    fn tampered_header_fails() {
        let v = UnlockedVault::create_with("correct horse battery", TEST_KDF).unwrap();
        let mut bytes = v.seal().unwrap();
        bytes[40] ^= 1; // Nonce-Byte kippen
        assert!(UnlockedVault::open(&bytes, "correct horse battery").is_err());
        let mut bytes = v.seal().unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1; // Tag kippen
        assert!(matches!(UnlockedVault::open(&bytes, "correct horse battery"), Err(Error::Decrypt)));
    }

    #[test]
    fn fresh_nonce_each_seal() {
        let v = UnlockedVault::create_with("correct horse battery", TEST_KDF).unwrap();
        assert_ne!(v.seal().unwrap(), v.seal().unwrap());
    }

    #[test]
    fn rejects_short_password() {
        assert!(UnlockedVault::create_with("short", TEST_KDF).is_err());
    }

    #[test]
    fn rejects_absurd_kdf_params() {
        let v = UnlockedVault::create_with("correct horse battery", TEST_KDF).unwrap();
        let mut bytes = v.seal().unwrap();
        bytes[9..13].copy_from_slice(&1000u32.to_le_bytes());
        assert!(matches!(UnlockedVault::open(&bytes, "correct horse battery"), Err(Error::Format)));
    }

    #[test]
    fn change_password_works() {
        let mut v = UnlockedVault::create_with("correct horse battery", TEST_KDF).unwrap();
        assert!(v.verify_password("correct horse battery").unwrap());
        assert!(!v.verify_password("something else!").unwrap());
        v.change_password("another long password", 1).unwrap();
        assert!(v.rekey_pending());
        let bytes = v.seal().unwrap();
        assert!(UnlockedVault::open(&bytes, "correct horse battery").is_err());
        assert!(UnlockedVault::open(&bytes, "another long password").is_ok());
    }

    #[test]
    fn atomic_write_creates_backup() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("vault.rk");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two");
        assert_eq!(fs::read(p.with_extension("rk.bak")).unwrap(), b"one");
    }
}
