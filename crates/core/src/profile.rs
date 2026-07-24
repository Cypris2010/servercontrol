use serde::{Deserialize, Serialize};
use url::Url;

/// Verbindungsprofil (Pflichtenheft 4.2). Enthält **kein** Passwort — nur den Verweis
/// (`credential_key`) in den OS-Credential-Store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerProfile {
    pub name: String,
    /// Adresse mit Schema `http` **oder** `https` (Kap. 8 LH: Schema folgenlos).
    pub base_url: Url,
    pub username: String,
    /// Credential-Store-Konto für den Web-Login (z. B. `<uuid>/web`).
    pub credential_key: String,
    /// Randfall gemietete Server: selbstsigniertes/ungültiges Zertifikat akzeptieren.
    #[serde(default)]
    pub accept_invalid_cert: bool,
    /// Optionaler FTP/SFTP-Zugang für große Uploads/Dateizugriff (MZ4).
    #[serde(default)]
    pub file_access: Option<FileAccess>,
}

/// FTP/SFTP-Zugang zum Serverordner (MZ4). Oft anderer Host/Port als das Web-Panel und mit
/// **eigenen** Zugangsdaten — daher ein eigener Credential-Store-Verweis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccess {
    pub protocol: FileProtocol,
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Credential-Store-Konto für den Datei-Zugang (z. B. `<uuid>/ftp`).
    pub credential_key: String,
    /// Pfad zum `mods/`-Ordner auf dem Server.
    pub mods_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileProtocol {
    Ftp,
    Sftp,
}
