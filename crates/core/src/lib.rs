//! Server Control for Farming Simulator 2025 — Kernbibliothek (`servercontrol-core`).
//!
//! Der **einzige Ort mit Logik**; CLI und GUI sind dünne, gleichberechtigte Schichten darauf.
//! Schnittstelle und Verhalten sind im Pflichtenheft festgelegt:
//! `docs/Pflichtenheft-ServerControl.md` (Kap. 4).
//!
//! Status: **Gerüst** — die Datentypen stehen, die Operationen sind Stubs (`todo!()`).

pub mod error;
pub mod model;
pub mod profile;
pub mod secret;
pub mod store;

// Interne Bausteine (Projektstruktur, Pflichtenheft Kap. 3).
mod credentials;
mod session;

// Noch umzusetzen.
mod control;
mod logs;
mod modhub;
mod mods;
mod settings;
mod verify;

use session::Session;

pub use error::Error;
pub use model::{
    Difficulty, FieldOption, GameSettings, LogChunk, LogListing, LogSource, ModStatus, OpCtx,
    PauseIfEmpty, Progress, ServerMod, ServerState, SettingsOptions,
};
pub use profile::{FileAccess, FileProtocol, ServerProfile};
pub use secret::Secret;
pub use store::AppSettings;
pub use uuid::Uuid as ProfileId;

/// Bequemer Ergebnistyp der Bibliothek.
pub type Result<T> = std::result::Result<T, Error>;

/// Passwort im OS-Credential-Store hinterlegen (Q1). Von CLI/GUI beim Einrichten eines
/// Profils genutzt; die Bibliothek liest es beim [`ServerControl::connect`] von dort und
/// hält es nie im Repo oder in Logs.
pub fn store_password(credential_key: &str, password: Secret) -> Result<()> {
    credentials::store(credential_key, &password)
}

/// Alle Profile laden (Kap. 8.1) — ohne Passwörter, die liegen getrennt im Credential-Store.
pub fn load_profiles() -> Result<Vec<ServerProfile>> {
    store::load_profiles()
}

/// Alle Profile speichern (Kap. 8.5, atomar).
pub fn save_profiles(profiles: &[ServerProfile]) -> Result<()> {
    store::save_profiles(profiles)
}

/// Anwendungseinstellungen laden (zuletzt aktives Profil, Theme, Sprache).
pub fn load_settings() -> Result<AppSettings> {
    store::load_settings()
}

/// Anwendungseinstellungen speichern.
pub fn save_settings(settings: &AppSettings) -> Result<()> {
    store::save_settings(settings)
}

/// Ob für einen Credential-Store-Schlüssel bereits ein Passwort hinterlegt ist — für die GUI
/// (Platzhaltertext „gespeichert" vs. „Passwort eingeben", Kap. 7.4), ohne das Passwort selbst
/// preiszugeben.
pub fn has_password(credential_key: &str) -> bool {
    credentials::load(credential_key).is_ok()
}

/// Ein Profil und seine Credential-Store-Einträge (Web + ggf. FTP/SFTP) vollständig entfernen
/// (Kap. 8.4: keine verwaisten Passwörter zurücklassen).
pub fn delete_profile_credentials(profile: &ServerProfile) -> Result<()> {
    credentials::delete(&ServerProfile::web_credential_key(profile.id))?;
    if profile.file_access.is_some() {
        credentials::delete(&ServerProfile::ftp_credential_key(profile.id))?;
    }
    Ok(())
}

/// Verbindung/Sitzung zu einem FS25-Dedicated-Server (Pflichtenheft 4.3).
///
/// Alle eingreifenden Operationen **verifizieren selbst** (Kap. 9) und geben erst bei
/// nachgewiesenem Ergebnis `Ok` zurück (`Error::NotProven` bei Zeitüberschreitung).
pub struct ServerControl {
    session: Session,
}

impl ServerControl {
    /// Anmelden, Cookie-Sitzung aufbauen (F2, Kap. 7.3 LH).
    ///
    /// Das Passwort wird zum `credential_key` aus dem OS-Credential-Store gelöst (Q1) und
    /// verlässt ihn nur im Moment des Login-POST. Version erkennen folgt mit [`Self::state`].
    pub async fn connect(profile: &ServerProfile, _ctx: &OpCtx) -> Result<Self> {
        let password = credentials::load(&profile.credential_key)?;
        let session = Session::login(
            profile.base_url.clone(),
            profile.accept_invalid_cert,
            profile.username.clone(),
            password,
        )
        .await?;
        Ok(Self { session })
    }

    /// Sitzung beenden (`index.html?logout=true`).
    pub async fn logout(self) -> Result<()> {
        self.session.logout().await
    }

    /// Aktueller Laufzeitzustand (online/offline + Spielversion).
    pub async fn state(&self) -> Result<ServerState> {
        self.session.state().await
    }

    // --- Logs (F7 / MZ5) ---

    /// Verfügbare Logs (Typen + Dateien) und die aktuelle `epoch` lesen.
    pub async fn list_logs(&self) -> Result<LogListing> {
        self.session.list_logs().await
    }

    /// Log inkrementell ab `offset` lesen (`tail -f` über HTTP). `epoch` aus [`Self::list_logs`].
    pub async fn read_log(
        &self,
        log_type: u8,
        log_file: &str,
        offset: u64,
        epoch: u64,
    ) -> Result<LogChunk> {
        self.session
            .read_log(log_type, log_file, offset, epoch)
            .await
    }

    // --- Mods (F3/F4) ---

    pub async fn list_mods(&self) -> Result<Vec<ServerMod>> {
        self.session.list_mods().await
    }

    /// Nur bei **gestopptem** Server möglich (sonst `Error::ServerRunning`).
    pub async fn set_active(&self, activate: &[String], deactivate: &[String]) -> Result<()> {
        self.session.set_active(activate, deactivate).await
    }

    /// Mod über das Web-Panel hochladen (bis 1,71 GB; darüber `NoFileAccess` → FTP nötig).
    pub async fn upload_mod(&self, path: &std::path::Path) -> Result<()> {
        self.session.upload_mod(path).await
    }

    /// Mod löschen — nur bei **gestopptem** Server (sonst `Error::ServerRunning`).
    pub async fn delete_mod(&self, file_name: &str) -> Result<()> {
        self.session.delete_mod(file_name).await
    }

    // --- Steuerung (F6) — Voll-Formular-Umlauf, Ergebnis am Zustand belegt ---

    pub async fn start(&self, _ctx: &OpCtx) -> Result<()> {
        self.session.start().await
    }
    pub async fn stop(&self, _ctx: &OpCtx) -> Result<()> {
        self.session.stop().await
    }
    pub async fn restart(&self, _ctx: &OpCtx) -> Result<()> {
        self.session.restart().await
    }

    // --- Spieleinstellungen (Kann/G6) ---

    pub async fn read_settings(&self) -> Result<GameSettings> {
        self.session.read_settings().await
    }

    /// Verfügbare Dropdown-Optionen der Einstellungen (Maps, Savegames, …) für die GUI (G6).
    pub async fn read_settings_options(&self) -> Result<SettingsOptions> {
        self.session.read_settings_options().await
    }
    pub async fn save_settings(&self, s: &GameSettings, _ctx: &OpCtx) -> Result<()> {
        self.session.save_settings(s).await
    }

    // Weitere Operationen (upload_mod, delete_mod, put_file/get_file/list_dir, Logs,
    // modhub_start/progress/cancel/download, catalog::search/details) — siehe
    // Pflichtenheft Kap. 4.3 / 4.4.
}
