//! Abgleich des Tresors mit Google Drive.
//!
//! Ablauf je Durchgang: Metadaten holen → bei neuer Remote-Version herunterladen,
//! entschlüsseln und per `VaultData::merge` einarbeiten → falls nötig hochladen.
//! Vor dem Hochladen wird geprüft, ob inzwischen ein anderes Gerät hochgeladen
//! hat; dann beginnt der Durchgang von vorn. Netzwerkzugriffe laufen nie unter
//! gehaltener Tresor-Sperre.

use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
use zeroize::Zeroizing;

use crate::error::{Error, Result};
use crate::gdrive::{self, Client, Tokens};
use crate::vault::{self, RemoteOpen, UnlockedVault, VaultData};
use crate::{now, AppState};

pub const STATUS_EVENT: &str = "sync-status";
pub const CHANGED_EVENT: &str = "vault-changed";
const INTERVAL: Duration = Duration::from_secs(60);
const DEBOUNCE: Duration = Duration::from_millis(1500);
const KEYRING_SERVICE: &str = "com.rememberkey.app";
const KEYRING_USER: &str = "google-drive-refresh-token";

pub enum Msg {
    /// Lokale Änderung gespeichert.
    Changed,
    /// Sofort abgleichen (z. B. nach dem Entsperren).
    Now,
}

/// Unverschlüsselt in `sync.json` neben dem Tresor – enthält keine Geheimnisse.
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncConfig {
    pub client_id: String,
    pub client_secret: String,
    pub connected: bool,
    pub remote_file_id: Option<String>,
    pub remote_version: Option<String>,
    pub last_sync: Option<i64>,
    /// Lokale Änderungen, die noch nicht hochgeladen wurden.
    pub pending: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    /// disabled | idle | syncing | offline | error | needs-password
    state: String,
    message: String,
    last_sync: Option<i64>,
    configured: bool,
    connected: bool,
    client_id: String,
}

pub struct SyncState {
    path: PathBuf,
    pub config: SyncConfig,
    tokens: Option<Tokens>,
    state: &'static str,
    message: String,
}

impl SyncState {
    pub fn new(path: PathBuf) -> Self {
        let config = fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { path, config, tokens: None, state: "idle", message: String::new() }
    }

    fn save(&self) -> Result<()> {
        fs::write(&self.path, serde_json::to_vec_pretty(&self.config)?)?;
        Ok(())
    }

    fn client(&self) -> Result<Client> {
        if self.config.client_id.trim().is_empty() || self.config.client_secret.trim().is_empty() {
            return Err(Error::Invalid("Bitte zuerst Client-ID und Client-Secret eintragen".into()));
        }
        Ok(Client {
            client_id: self.config.client_id.trim().to_string(),
            client_secret: self.config.client_secret.trim().to_string(),
        })
    }

    pub fn status(&self) -> SyncStatus {
        SyncStatus {
            state: if self.config.connected { self.state.into() } else { "disabled".into() },
            message: self.message.clone(),
            last_sync: self.config.last_sync,
            configured: self.client().is_ok(),
            connected: self.config.connected,
            client_id: self.config.client_id.clone(),
        }
    }
}

// ---------- Anmeldeinformationen (Windows-Anmeldeinformationsverwaltung) ----------

fn keyring_entry() -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| Error::Sync(format!("Anmeldeinformationen: {e}")))
}

fn store_refresh(token: &str) -> Result<()> {
    keyring_entry()?.set_password(token).map_err(|e| Error::Sync(format!("Anmeldeinformationen: {e}")))
}

fn load_refresh() -> Result<Zeroizing<String>> {
    match keyring_entry()?.get_password() {
        Ok(t) => Ok(Zeroizing::new(t)),
        Err(keyring::Error::NoEntry) => Err(Error::SyncAuth),
        Err(e) => Err(Error::Sync(format!("Anmeldeinformationen: {e}"))),
    }
}

fn delete_refresh() {
    if let Ok(e) = keyring_entry() {
        let _ = e.delete_credential();
    }
}

// ---------- Hilfen ----------

fn set_status(app: &AppHandle, state: &'static str, message: &str) {
    let st = app.state::<AppState>();
    let status = {
        let mut s = st.sync_lock();
        s.state = state;
        s.message = message.to_string();
        s.status()
    };
    let _ = app.emit(STATUS_EVENT, status);
}

fn emit_status(app: &AppHandle) {
    let status = app.state::<AppState>().sync_lock().status();
    let _ = app.emit(STATUS_EVENT, status);
}

/// Gültiges Access-Token aus dem Speicher oder per Refresh-Token neu.
fn access_token(app: &AppHandle) -> Result<Zeroizing<String>> {
    let st = app.state::<AppState>();
    let client = {
        let s = st.sync_lock();
        if let Some(t) = s.tokens.as_ref().filter(|t| t.valid()) {
            return Ok(t.access.clone());
        }
        s.client()?
    };
    let refresh = load_refresh()?;
    let tokens = gdrive::refresh(&client, &refresh)?;
    if let Some(r) = &tokens.refresh {
        store_refresh(r)?;
    }
    let access = tokens.access.clone();
    st.sync_lock().tokens = Some(tokens);
    Ok(access)
}

pub fn is_connected(app: &AppHandle) -> bool {
    app.state::<AppState>().sync_lock().config.connected
}

// ---------- Öffentliche Aktionen ----------

pub fn configure(app: &AppHandle, client_id: String, client_secret: String) -> Result<()> {
    let st = app.state::<AppState>();
    {
        let mut s = st.sync_lock();
        if s.config.connected {
            return Err(Error::Invalid("Bitte zuerst die Verbindung zu Google Drive trennen".into()));
        }
        s.config.client_id = client_id.trim().to_string();
        s.config.client_secret = client_secret.trim().to_string();
        s.save()?;
    }
    emit_status(app);
    Ok(())
}

pub fn connect(app: &AppHandle) -> Result<()> {
    let st = app.state::<AppState>();
    let client = st.sync_lock().client()?;
    let tokens = gdrive::authorize(&client, |url| {
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|e| Error::Sync(format!("Browser konnte nicht geöffnet werden: {e}")))
    })?;
    if let Some(r) = &tokens.refresh {
        store_refresh(r)?;
    }
    {
        let mut s = st.sync_lock();
        s.tokens = Some(tokens);
        s.config.connected = true;
        s.config.remote_file_id = None;
        s.config.remote_version = None;
        s.config.last_sync = None;
        s.config.pending = true;
        s.state = "idle";
        s.message.clear();
        s.save()?;
    }
    emit_status(app);
    Ok(())
}

pub fn disconnect(app: &AppHandle) -> Result<()> {
    let st = app.state::<AppState>();
    if let Ok(r) = load_refresh() {
        gdrive::revoke(&r);
    }
    delete_refresh();
    {
        let mut s = st.sync_lock();
        s.tokens = None;
        s.config.connected = false;
        s.config.remote_file_id = None;
        s.config.remote_version = None;
        s.config.last_sync = None;
        s.config.pending = false;
        s.state = "idle";
        s.message.clear();
        s.save()?;
    }
    emit_status(app);
    Ok(())
}

/// Lädt den Tresor aus Google Drive (z. B. auf einem neuen Gerät).
pub fn download_remote(app: &AppHandle) -> Result<Option<(gdrive::RemoteFile, Vec<u8>)>> {
    let token = access_token(app)?;
    let Some(remote) = gdrive::find(&token)? else { return Ok(None) };
    let bytes = gdrive::download(&token, &remote.id)?;
    if !vault::looks_like_vault(&bytes) {
        return Err(Error::Sync("Die Datei in Google Drive ist kein gültiger Tresor".into()));
    }
    Ok(Some((remote, bytes)))
}

/// Merkt sich die Remote-Version, deren Inhalt lokal vollständig übernommen wurde.
pub fn remember_remote(app: &AppHandle, remote: &gdrive::RemoteFile) -> Result<()> {
    let st = app.state::<AppState>();
    let mut s = st.sync_lock();
    s.config.remote_file_id = Some(remote.id.clone());
    s.config.remote_version = Some(remote.version.clone());
    s.config.last_sync = Some(now());
    s.config.pending = false;
    s.save()
}

/// Einzelner Abgleich mit Statusmeldungen. `password` nur nötig, wenn der Stand in
/// Google Drive mit anderem Salt verschlüsselt ist (anderes Gerät, Passwortwechsel).
pub fn run(app: &AppHandle, password: Option<&str>) -> Result<()> {
    let st = app.state::<AppState>();
    {
        let s = st.sync_lock();
        if !s.config.connected {
            return Ok(());
        }
        // Ohne Passwort ändert ein erneuter Versuch nichts – Netzwerk sparen.
        if password.is_none() && s.state == "needs-password" {
            return Err(Error::NeedsPassword);
        }
    }
    if st.lock().vault.is_none() {
        return Ok(());
    }
    let _running = st.sync_running.lock().unwrap_or_else(|e| e.into_inner());
    set_status(app, "syncing", "");
    let result = sync_once(app, password);
    match &result {
        Ok(changed) => {
            set_status(app, "idle", "");
            if *changed {
                let _ = app.emit(CHANGED_EVENT, ());
            }
        }
        Err(Error::Locked) => set_status(app, "idle", ""),
        Err(Error::NeedsPassword) => set_status(
            app,
            "needs-password",
            "Der Tresor in Google Drive wurde mit einem anderen Schlüssel gespeichert. Bitte Master-Passwort bestätigen.",
        ),
        Err(Error::Offline(_)) => set_status(app, "offline", "Keine Verbindung zu Google Drive"),
        Err(e) => set_status(app, "error", &e.to_string()),
    }
    result.map(|_| ())
}

/// Hält das zum Zusammenführen geöffnete Gegenstück.
enum Source {
    Data(VaultData),
    Vault(UnlockedVault),
}

impl Source {
    fn data(&self) -> &VaultData {
        match self {
            Source::Data(d) => d,
            Source::Vault(v) => &v.data,
        }
    }
}

/// Gibt zurück, ob sich der lokale Tresor durch den Abgleich geändert hat.
fn sync_once(app: &AppHandle, password: Option<&str>) -> Result<bool> {
    let st = app.state::<AppState>();
    let mut changed_local = false;

    for _attempt in 0..3 {
        let token = access_token(app)?;
        let (file_id, known_version, pending) = {
            let s = st.sync_lock();
            (s.config.remote_file_id.clone(), s.config.remote_version.clone(), s.config.pending)
        };
        let remote = match &file_id {
            Some(id) => gdrive::metadata(&token, id)?,
            None => None,
        };
        let remote = match remote {
            Some(r) => Some(r),
            None => gdrive::find(&token)?,
        };

        // Lokaler Schlüsselwechsel (Passwortwechsel, KDF-Umstellung) noch nicht in Drive?
        let mut rekey = {
            let g = st.lock();
            g.vault.as_ref().ok_or(Error::Locked)?.rekey_pending()
        };

        // 1. Neuen Stand aus Google Drive einarbeiten.
        let mut need_upload = remote.is_none() || pending || rekey;
        if let Some(r) = &remote {
            if known_version.as_deref() != Some(r.version.as_str()) {
                let bytes = gdrive::download(&token, &r.id)?;
                let opened = {
                    let mut g = st.lock();
                    let v = g.vault.as_mut().ok_or(Error::Locked)?;
                    if rekey && v.is_current_key_file(&bytes) {
                        // Drive hat den neuen Schlüssel bereits (z. B. abgebrochener Durchgang).
                        v.finish_rekey();
                        rekey = false;
                    }
                    v.open_remote(&bytes)?
                };
                let source = match opened {
                    RemoteOpen::Data(d) => Source::Data(d),
                    RemoteOpen::NeedsPassword => {
                        let pw = password.ok_or(Error::NeedsPassword)?;
                        // Argon2 ohne gehaltene Sperre.
                        let rv = UnlockedVault::open(&bytes, pw).map_err(|e| match e {
                            Error::Decrypt => Error::Sync(
                                "Der Tresor in Google Drive ist mit einem anderen Master-Passwort verschlüsselt".into(),
                            ),
                            other => other,
                        })?;
                        Source::Vault(rv)
                    }
                };

                let mut g = st.lock();
                let v = g.vault.as_mut().ok_or(Error::Locked)?;
                let before = vault::to_json_zeroizing(&v.data)?;
                let mut adopted = false;
                if let Source::Vault(rv) = &source {
                    // Wessen Schlüssel gilt? Der jüngere Schlüsselwechsel gewinnt.
                    if rv.data.key_changed_at >= v.data.key_changed_at {
                        v.adopt_key(rv);
                        adopted = true;
                        rekey = false;
                    } else {
                        rekey = true;
                    }
                }
                v.data.merge(source.data());
                let after = vault::to_json_zeroizing(&v.data)?;
                let remote_json = vault::to_json_zeroizing(source.data())?;
                if *after != *before || adopted {
                    g.write_local()?;
                    changed_local = *after != *before;
                }
                // Hochladen nur, wenn der zusammengeführte Stand vom Remote-Stand abweicht.
                need_upload = rekey || *after != *remote_json;
            }
        }

        // 2. Lokalen Stand hochladen.
        let uploaded = if need_upload {
            let bytes = {
                let g = st.lock();
                g.vault.as_ref().ok_or(Error::Locked)?.seal()?
            };
            if let Some(r) = &remote {
                // Hat ein anderes Gerät inzwischen hochgeladen? Dann erneut zusammenführen.
                let current = gdrive::metadata(&token, &r.id)?;
                if current.as_ref().map(|c| c.version.as_str()) != Some(r.version.as_str()) {
                    let mut s = st.sync_lock();
                    if s.config.remote_version.as_deref() == Some(r.version.as_str()) {
                        s.config.remote_version = None; // erzwingt Download im nächsten Durchgang
                    }
                    continue;
                }
            }
            match (&remote, rekey) {
                (Some(r), false) => gdrive::update(&token, &r.id, &bytes)?,
                _ => {
                    // Neue Datei statt Update: Google Drive hebt alte Versionen einer Datei
                    // auf. Beim Schlüsselwechsel wird die alte Datei samt Versionen gelöscht,
                    // damit kein Stand mit dem alten Passwort zurückbleibt.
                    let created = gdrive::create(&token, &bytes)?;
                    if rekey {
                        for f in gdrive::find_all(&token)? {
                            if f.id != created.id {
                                gdrive::delete(&token, &f.id)?;
                            }
                        }
                        if let Some(v) = st.lock().vault.as_mut() {
                            v.finish_rekey();
                        }
                    }
                    created
                }
            }
        } else {
            match remote {
                Some(r) => r,
                None => return Ok(changed_local),
            }
        };

        let mut s = st.sync_lock();
        s.config.remote_file_id = Some(uploaded.id);
        s.config.remote_version = Some(uploaded.version);
        s.config.last_sync = Some(now());
        s.config.pending = false;
        s.save()?;
        return Ok(changed_local);
    }
    Err(Error::Sync("Google Drive wurde gleichzeitig von einem anderen Gerät geändert. Bitte erneut versuchen.".into()))
}

/// Hintergrund-Thread: gleicht nach Änderungen (entprellt) und alle 60 s ab.
pub fn spawn_worker(app: AppHandle, rx: Receiver<Msg>) {
    thread::spawn(move || loop {
        match rx.recv_timeout(INTERVAL) {
            Ok(Msg::Changed) => {
                {
                    let st = app.state::<AppState>();
                    let mut s = st.sync_lock();
                    if s.config.connected && !s.config.pending {
                        s.config.pending = true;
                        let _ = s.save();
                    }
                }
                thread::sleep(DEBOUNCE);
                while rx.try_recv().is_ok() {}
            }
            Ok(Msg::Now) => while rx.try_recv().is_ok() {},
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let _ = run(&app, None);
    });
}
