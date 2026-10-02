use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Tresor ist gesperrt")]
    Locked,
    #[error("Es existiert noch kein Tresor")]
    NoVault,
    #[error("Es existiert bereits ein Tresor")]
    VaultExists,
    #[error("Falsches Master-Passwort oder beschädigte Tresor-Datei")]
    Decrypt,
    #[error("Ungültiges Tresor-Format")]
    Format,
    #[error("Eintrag nicht gefunden")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error("Zwischenablage: {0}")]
    Clipboard(String),
    #[error("Google Drive: {0}")]
    Sync(String),
    #[error("Keine Verbindung zu Google Drive ({0})")]
    Offline(String),
    #[error("Die Google-Anmeldung ist abgelaufen oder wurde widerrufen. Bitte in den Einstellungen neu verbinden.")]
    SyncAuth,
    #[error("Zum Abgleich mit Google Drive wird das Master-Passwort benötigt")]
    NeedsPassword,
    #[error("Dateifehler: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialisierungsfehler: {0}")]
    Json(#[from] serde_json::Error),
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
