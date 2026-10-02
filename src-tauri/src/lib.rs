mod clipboard;
mod error;
mod gdrive;
mod generator;
mod session;
mod strength;
mod sync;
mod totp;
mod vault;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use error::{Error, Result};
use generator::{GenOptions, Generated};
use vault::{Entry, Settings, UnlockedVault};

const LOCKED_EVENT: &str = "vault-locked";

struct Inner {
    path: PathBuf,
    vault: Option<UnlockedVault>,
    last_activity: Instant,
    /// Sequenznummer der Zwischenablage nach unserem letzten Kopieren.
    clipboard_seq: Option<u32>,
    sync_tx: Sender<sync::Msg>,
}

/// Sperrreihenfolge: `inner` vor `sync`. Netzwerkzugriffe nie unter `inner`.
pub(crate) struct AppState {
    inner: Mutex<Inner>,
    sync: Mutex<sync::SyncState>,
    /// Verhindert parallele Sync-Durchgänge (Hintergrund + manuell).
    sync_running: Mutex<()>,
}

impl AppState {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn sync_lock(&self) -> MutexGuard<'_, sync::SyncState> {
        self.sync.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Inner {
    fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    fn vault(&mut self) -> Result<&mut UnlockedVault> {
        self.touch();
        self.vault.as_mut().ok_or(Error::Locked)
    }

    /// Speichert lokal und stößt den Abgleich an.
    fn persist(&self) -> Result<()> {
        self.write_local()?;
        let _ = self.sync_tx.send(sync::Msg::Changed);
        Ok(())
    }

    /// Speichert nur lokal (z. B. nach dem Zusammenführen mit Google Drive).
    fn write_local(&self) -> Result<()> {
        let v = self.vault.as_ref().ok_or(Error::Locked)?;
        vault::write_atomic(&self.path, &v.seal()?)
    }

    fn lock_vault(&mut self) {
        self.vault = None; // Drop nullt Schlüssel und Inhalte
        if let Some(seq) = self.clipboard_seq.take() {
            clipboard::clear_if_unchanged(seq);
        }
    }
}

/// Löscht lokale Sicherungskopien, die noch mit einem alten Schlüssel verschlüsselt sind.
fn delete_backups(path: &Path) {
    let _ = std::fs::remove_file(path.with_extension("rk.bak"));
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else { return };
    let prefix = format!("{name}.vor-sync-");
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// Sperrt bei Windows-Sperre oder Energiesparmodus (siehe `session`).
pub(crate) fn lock_from_system(app: &AppHandle) {
    let st = app.state::<AppState>();
    let mut g = st.lock();
    if g.vault.is_some() {
        g.lock_vault();
        drop(g);
        let _ = app.emit(LOCKED_EVENT, "system");
    }
}

pub(crate) fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

// ---------- DTOs ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VaultStatus {
    exists: bool,
    unlocked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EntrySummary {
    id: String,
    title: String,
    username: String,
    url: String,
    has_totp: bool,
    folder_id: Option<String>,
    updated_at: i64,
}

/// Das Passwort selbst wird nur auf Anforderung (`reveal_password`) übertragen.
#[derive(Serialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct EntryDetail {
    id: String,
    title: String,
    username: String,
    has_password: bool,
    url: String,
    notes: String,
    totp: String,
    folder_id: Option<String>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Deserialize, Zeroize)]
#[serde(rename_all = "camelCase")]
#[zeroize(drop)]
struct EntryInput {
    id: Option<String>,
    title: String,
    username: String,
    /// `None` = beim Bearbeiten unverändert lassen.
    password: Option<String>,
    url: String,
    notes: String,
    totp: String,
    folder_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderDto {
    id: String,
    name: String,
    parent_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderInput {
    id: Option<String>,
    name: String,
    parent_id: Option<String>,
}

#[derive(Serialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
struct SecretString(String);

#[derive(Serialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
struct TotpCode {
    code: String,
    remaining: u64,
    period: u64,
}

// ---------- Tresor-Lebenszyklus ----------

#[tauri::command]
fn vault_status(state: State<'_, AppState>) -> VaultStatus {
    let g = state.lock();
    VaultStatus { exists: g.path.exists(), unlocked: g.vault.is_some() }
}

#[tauri::command]
async fn create_vault(state: State<'_, AppState>, password: String) -> Result<()> {
    let password = Zeroizing::new(password);
    if state.lock().path.exists() {
        return Err(Error::VaultExists);
    }
    strength::check_master(&password)?;
    let mut v = UnlockedVault::create(&password)?;
    v.data.key_changed_at = now();
    let mut g = state.lock();
    g.vault = Some(v);
    g.touch();
    g.persist()
}

#[tauri::command]
async fn unlock(app: AppHandle, state: State<'_, AppState>, password: String) -> Result<()> {
    let password = Zeroizing::new(password);
    let path = state.lock().path.clone();
    if !path.exists() {
        return Err(Error::NoVault);
    }
    let bytes = std::fs::read(&path)?;
    // Argon2 läuft ohne gehaltenen Mutex, damit die Auto-Sperre nicht blockiert.
    let v = match UnlockedVault::open(&bytes, &password) {
        Ok(v) => v,
        // Master-Passwort evtl. auf einem anderen Gerät geändert: Stand aus Google Drive versuchen.
        Err(Error::Decrypt) if sync::is_connected(&app) => {
            let Some((remote, remote_bytes)) = sync::download_remote(&app).ok().flatten() else {
                return Err(Error::Decrypt);
            };
            let rv = UnlockedVault::open(&remote_bytes, &password)?;
            std::fs::copy(&path, path.with_extension(format!("rk.vor-sync-{}.bak", now())))?;
            vault::write_atomic(&path, &remote_bytes)?;
            sync::remember_remote(&app, &remote)?;
            rv
        }
        Err(e) => return Err(e),
    };
    // Ältere, schwächere Argon2-Parameter automatisch anheben (Passwort bleibt gleich).
    let mut v = v;
    let upgraded = v.needs_kdf_upgrade();
    if upgraded {
        v.upgrade_kdf(&password, now())?;
    }
    let mut g = state.lock();
    g.vault = Some(v);
    g.touch();
    if upgraded {
        g.persist()?;
        delete_backups(&path);
    }
    let _ = g.sync_tx.send(sync::Msg::Now);
    Ok(())
}

#[tauri::command]
fn lock(state: State<'_, AppState>) {
    state.lock().lock_vault();
}

#[tauri::command]
fn touch(state: State<'_, AppState>) {
    state.lock().touch();
}

#[tauri::command]
async fn change_master_password(
    state: State<'_, AppState>,
    current: String,
    new_password: String,
) -> Result<()> {
    let current = Zeroizing::new(current);
    let new_password = Zeroizing::new(new_password);
    strength::check_master(&new_password)?;
    let mut g = state.lock();
    let v = g.vault()?;
    if !v.verify_password(&current)? {
        return Err(Error::Invalid("Aktuelles Master-Passwort ist falsch".into()));
    }
    v.change_password(&new_password, now())?;
    g.persist()?;
    // Sicherungen mit dem alten Passwort entfernen; die Kopie in Google Drive wird
    // beim nächsten Abgleich durch eine neue Datei ersetzt (siehe sync::sync_once).
    delete_backups(&g.path);
    Ok(())
}

#[tauri::command]
fn password_strength(password: String) -> strength::Strength {
    strength::evaluate(&Zeroizing::new(password))
}

// ---------- Einträge ----------

#[tauri::command]
fn list_entries(state: State<'_, AppState>) -> Result<Vec<EntrySummary>> {
    let mut g = state.lock();
    let mut list: Vec<EntrySummary> = g
        .vault()?
        .data
        .entries
        .iter()
        .map(|e| EntrySummary {
            id: e.id.clone(),
            title: e.title.clone(),
            username: e.username.clone(),
            url: e.url.clone(),
            has_totp: e.totp.is_some(),
            folder_id: e.folder_id.clone(),
            updated_at: e.updated_at,
        })
        .collect();
    list.sort_by_key(|e| e.title.to_lowercase());
    Ok(list)
}

#[tauri::command]
fn get_entry(state: State<'_, AppState>, id: String) -> Result<EntryDetail> {
    let mut g = state.lock();
    let e = g.vault()?.data.entries.iter().find(|e| e.id == id).ok_or(Error::NotFound)?;
    Ok(EntryDetail {
        id: e.id.clone(),
        title: e.title.clone(),
        username: e.username.clone(),
        has_password: !e.password.is_empty(),
        url: e.url.clone(),
        notes: e.notes.clone(),
        totp: e.totp.as_ref().map(totp::to_input).unwrap_or_default(),
        folder_id: e.folder_id.clone(),
        created_at: e.created_at,
        updated_at: e.updated_at,
    })
}

#[tauri::command]
fn reveal_password(state: State<'_, AppState>, id: String) -> Result<SecretString> {
    let mut g = state.lock();
    let e = g.vault()?.data.entries.iter().find(|e| e.id == id).ok_or(Error::NotFound)?;
    Ok(SecretString(e.password.clone()))
}

#[tauri::command]
fn save_entry(state: State<'_, AppState>, entry: EntryInput) -> Result<String> {
    if entry.title.trim().is_empty() {
        return Err(Error::Invalid("Titel darf nicht leer sein".into()));
    }
    let totp_cfg = match entry.totp.trim() {
        "" => None,
        s => Some(totp::parse_input(s)?),
    };
    let ts = now();
    let mut g = state.lock();
    let v = g.vault()?;
    v.data.check_entry_folder(entry.folder_id.as_deref())?;
    let id = match &entry.id {
        Some(id) => {
            let e = v.data.entries.iter_mut().find(|e| &e.id == id).ok_or(Error::NotFound)?;
            e.title = entry.title.trim().to_string();
            e.username = entry.username.clone();
            if let Some(pw) = &entry.password {
                e.password = pw.clone();
            }
            e.url = entry.url.trim().to_string();
            e.notes = entry.notes.clone();
            e.totp = totp_cfg;
            e.folder_id = entry.folder_id.clone();
            e.updated_at = ts;
            id.clone()
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            v.data.entries.push(Entry {
                id: id.clone(),
                title: entry.title.trim().to_string(),
                username: entry.username.clone(),
                password: entry.password.clone().unwrap_or_default(),
                url: entry.url.trim().to_string(),
                notes: entry.notes.clone(),
                totp: totp_cfg,
                folder_id: entry.folder_id.clone(),
                created_at: ts,
                updated_at: ts,
            });
            id
        }
    };
    g.persist()?;
    Ok(id)
}

#[tauri::command]
fn delete_entry(state: State<'_, AppState>, id: String) -> Result<()> {
    let mut g = state.lock();
    g.vault()?.data.delete_entry(&id, now())?;
    g.persist()
}

#[tauri::command]
fn move_entry(state: State<'_, AppState>, id: String, folder_id: Option<String>) -> Result<()> {
    let mut g = state.lock();
    let v = g.vault()?;
    v.data.check_entry_folder(folder_id.as_deref())?;
    let e = v.data.entries.iter_mut().find(|e| e.id == id).ok_or(Error::NotFound)?;
    e.folder_id = folder_id;
    e.updated_at = now();
    g.persist()
}

// ---------- Ordner ----------

#[tauri::command]
fn list_folders(state: State<'_, AppState>) -> Result<Vec<FolderDto>> {
    let mut g = state.lock();
    let mut list: Vec<FolderDto> = g
        .vault()?
        .data
        .folders
        .iter()
        .map(|f| FolderDto { id: f.id.clone(), name: f.name.clone(), parent_id: f.parent_id.clone() })
        .collect();
    list.sort_by_key(|f| f.name.to_lowercase());
    Ok(list)
}

#[tauri::command]
fn save_folder(state: State<'_, AppState>, folder: FolderInput) -> Result<String> {
    let mut g = state.lock();
    let data = &mut g.vault()?.data;
    let id = match folder.id {
        Some(id) => {
            data.update_folder(&id, &folder.name, folder.parent_id, now())?;
            id
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            data.create_folder(id.clone(), &folder.name, folder.parent_id, now())?;
            id
        }
    };
    g.persist()?;
    Ok(id)
}

#[tauri::command]
fn delete_folder(state: State<'_, AppState>, id: String) -> Result<()> {
    let mut g = state.lock();
    g.vault()?.data.delete_folder(&id, now())?;
    g.persist()
}

/// Wird sekündlich abgefragt und zählt deshalb bewusst nicht als Aktivität.
#[tauri::command]
fn totp_code(state: State<'_, AppState>, id: String) -> Result<Option<TotpCode>> {
    let g = state.lock();
    let v = g.vault.as_ref().ok_or(Error::Locked)?;
    let e = v.data.entries.iter().find(|e| e.id == id).ok_or(Error::NotFound)?;
    let Some(cfg) = &e.totp else { return Ok(None) };
    let (code, remaining) = totp::generate(cfg, now() as u64)?;
    Ok(Some(TotpCode { code, remaining, period: cfg.period }))
}

/// Kopiert Benutzername, Passwort oder TOTP-Code, ohne dass das Geheimnis
/// über das Frontend laufen muss. Gibt die Sekunden bis zum Leeren zurück.
#[tauri::command]
fn copy_field(state: State<'_, AppState>, id: String, field: String) -> Result<u32> {
    let mut g = state.lock();
    let v = g.vault()?;
    let clear_after = v.data.settings.clipboard_clear_seconds;
    let e = v.data.entries.iter().find(|e| e.id == id).ok_or(Error::NotFound)?;
    let value = Zeroizing::new(match field.as_str() {
        "username" => e.username.clone(),
        "password" => e.password.clone(),
        "totp" => {
            let cfg = e.totp.as_ref().ok_or(Error::Invalid("Kein TOTP hinterlegt".into()))?;
            totp::generate(cfg, now() as u64)?.0
        }
        _ => return Err(Error::Invalid("Unbekanntes Feld".into())),
    });
    let seq = clipboard::set_secret(&value)?;
    g.clipboard_seq = Some(seq);
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(clear_after as u64));
        clipboard::clear_if_unchanged(seq);
    });
    Ok(clear_after)
}

/// Kopiert einen frei übergebenen Wert (z. B. frisch generiertes Passwort).
#[tauri::command]
fn copy_text(state: State<'_, AppState>, text: String) -> Result<u32> {
    let text = Zeroizing::new(text);
    let mut g = state.lock();
    let clear_after = g.vault()?.data.settings.clipboard_clear_seconds;
    let seq = clipboard::set_secret(&text)?;
    g.clipboard_seq = Some(seq);
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(clear_after as u64));
        clipboard::clear_if_unchanged(seq);
    });
    Ok(clear_after)
}

#[tauri::command]
fn generate_password(options: GenOptions) -> Result<Generated> {
    generator::generate(&options)
}

// ---------- Einstellungen ----------

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    Ok(state.lock().vault()?.data.settings)
}

#[tauri::command]
fn set_settings(state: State<'_, AppState>, mut settings: Settings) -> Result<()> {
    settings.validate()?;
    settings.updated_at = now();
    let mut g = state.lock();
    g.vault()?.data.settings = settings;
    g.persist()
}

// ---------- Google Drive ----------

#[tauri::command]
fn sync_status(state: State<'_, AppState>) -> sync::SyncStatus {
    state.sync_lock().status()
}

#[tauri::command]
fn sync_configure(app: AppHandle, client_id: String, client_secret: String) -> Result<()> {
    sync::configure(&app, client_id, client_secret)
}

/// Öffnet den Google-Login im Browser und wartet bis zu drei Minuten.
#[tauri::command]
async fn sync_connect(app: AppHandle) -> Result<()> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync::connect(&handle))
        .await
        .map_err(|e| Error::Sync(e.to_string()))??;
    let _ = app.state::<AppState>().lock().sync_tx.send(sync::Msg::Now);
    Ok(())
}

#[tauri::command]
async fn sync_disconnect(app: AppHandle) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || sync::disconnect(&app))
        .await
        .map_err(|e| Error::Sync(e.to_string()))?
}

#[tauri::command]
async fn sync_now(app: AppHandle, password: Option<String>) -> Result<()> {
    let password = password.map(Zeroizing::new);
    tauri::async_runtime::spawn_blocking(move || sync::run(&app, password.as_deref().map(|p| p.as_str())))
        .await
        .map_err(|e| Error::Sync(e.to_string()))?
}

/// Neues Gerät: vorhandenen Tresor aus Google Drive als lokale Datei übernehmen.
#[tauri::command]
async fn sync_restore(app: AppHandle) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = app.state::<AppState>().lock().path.clone();
        if path.exists() {
            return Err(Error::VaultExists);
        }
        let (remote, bytes) = sync::download_remote(&app)?
            .ok_or_else(|| Error::Sync("In Google Drive wurde noch kein Tresor gefunden".into()))?;
        vault::write_atomic(&path, &bytes)?;
        sync::remember_remote(&app, &remote)
    })
    .await
    .map_err(|e| Error::Sync(e.to_string()))?
}

// ---------- Auto-Sperre ----------

fn spawn_auto_lock(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(2));
        let state = app.state::<AppState>();
        let mut g = state.lock();
        let Some(v) = &g.vault else { continue };
        let timeout = Duration::from_secs(v.data.settings.auto_lock_minutes as u64 * 60);
        if g.last_activity.elapsed() >= timeout {
            g.lock_vault();
            drop(g);
            let _ = app.emit(LOCKED_EVENT, "idle");
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let (sync_tx, sync_rx) = mpsc::channel();
            app.manage(AppState {
                inner: Mutex::new(Inner {
                    path: dir.join("vault.rk"),
                    vault: None,
                    last_activity: Instant::now(),
                    clipboard_seq: None,
                    sync_tx,
                }),
                sync: Mutex::new(sync::SyncState::new(dir.join("sync.json"))),
                sync_running: Mutex::new(()),
            });
            spawn_auto_lock(app.handle().clone());
            session::spawn(app.handle().clone());
            sync::spawn_worker(app.handle().clone(), sync_rx);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            vault_status,
            create_vault,
            unlock,
            lock,
            touch,
            change_master_password,
            list_entries,
            get_entry,
            reveal_password,
            password_strength,
            save_entry,
            delete_entry,
            move_entry,
            list_folders,
            save_folder,
            delete_folder,
            totp_code,
            copy_field,
            copy_text,
            generate_password,
            get_settings,
            set_settings,
            sync_status,
            sync_configure,
            sync_connect,
            sync_disconnect,
            sync_now,
            sync_restore,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(state) = handle.try_state::<AppState>() {
                state.lock().lock_vault();
            }
        }
    });
}
