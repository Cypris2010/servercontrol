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
mod modfile;
mod modhub;
mod mods;
mod savegames;
mod settings;
mod verify;

use session::Session;

pub use error::Error;
pub use model::{
    CatalogDetails, CatalogEntry, Difficulty, FieldOption, GameSettings, LogChunk, LogListing,
    LogSource, ModStatus, ModhubCategoryEntry, OpCtx, PauseIfEmpty, Progress, SavegameBackup,
    ServerMod, ServerSavegame, ServerState, SettingsOptions, SettingsRow,
};
pub use modfile::{inspect_local_mod, LocalModInfo};
pub use modhub::catalog;
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

    /// Wie [`Self::upload_mod`], meldet aber laufend den Fortschritt (`progress`-Events, Kap. 7.3).
    pub async fn upload_mod_with_progress<F>(
        &self,
        path: &std::path::Path,
        on_progress: F,
    ) -> Result<()>
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        self.session
            .upload_mod_with_progress(path, on_progress)
            .await
    }

    /// Mod löschen — nur bei **gestopptem** Server (sonst `Error::ServerRunning`).
    pub async fn delete_mod(&self, file_name: &str) -> Result<()> {
        self.session.delete_mod(file_name).await
    }

    // --- Savegames (Kann, Kap. 1.3 LH / 7.8 LH) ---
    //
    // Anders als Mods/Einstellungen **keine** Sperre durch den Serverzustand — verifiziert:
    // Upload/Löschen/Restore gehen bei laufendem wie bei gestopptem Server.

    /// Belegte Savegame-Slots lesen.
    pub async fn list_savegames(&self) -> Result<Vec<ServerSavegame>> {
        self.session.list_savegames().await
    }

    /// Ziel-Slots des Upload-Formulars lesen — die echten Formular-Optionen (fehlt darin das
    /// aktuell geladene Savegame, verifiziert), nicht synthetisch 1..20 nachgebaut.
    pub async fn list_savegame_upload_slots(&self) -> Result<Vec<FieldOption>> {
        self.session.list_savegame_upload_slots().await
    }

    /// Zeitstempel-Backups eines Slots lesen.
    pub async fn list_savegame_backups(&self, slot: u8) -> Result<Vec<SavegameBackup>> {
        self.session.list_savegame_backups(slot).await
    }

    /// Savegame eines Slots herunterladen (einfacher Datei-Download, kein Q3-Nachweis nötig —
    /// rein lesend).
    pub async fn download_savegame(&self, slot: u8, local: &std::path::Path) -> Result<()> {
        self.session.download_savegame(slot, local).await
    }

    /// Savegame über das Web-Formular hochladen (bis 1,71 GB; darüber `NoFileAccess`).
    pub async fn upload_savegame(
        &self,
        slot: u8,
        name: Option<&str>,
        path: &std::path::Path,
    ) -> Result<()> {
        self.session.upload_savegame(slot, name, path).await
    }

    /// Wie [`Self::upload_savegame`], meldet aber laufend den Fortschritt (`progress`-Events).
    pub async fn upload_savegame_with_progress<F>(
        &self,
        slot: u8,
        name: Option<&str>,
        path: &std::path::Path,
        on_progress: F,
    ) -> Result<()>
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        self.session
            .upload_savegame_with_progress(slot, name, path, on_progress)
            .await
    }

    /// Savegame löschen — destruktiv.
    pub async fn delete_savegame(&self, slot: u8) -> Result<()> {
        self.session.delete_savegame(slot).await
    }

    /// Zeitstempel-Backup wiederherstellen — überschreibt den Slot vollständig.
    pub async fn restore_savegame_backup(&self, backup: &SavegameBackup) -> Result<()> {
        self.session.restore_savegame_backup(backup).await
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

    /// Einstellungen als Textanzeige (G6, nur bei **laufendem** Server — dort liefert
    /// [`Self::read_settings`] `Error::FormMismatch`, weil das Panel dort kein Formular zeigt).
    pub async fn read_settings_summary(&self) -> Result<Vec<SettingsRow>> {
        self.session.read_settings_summary().await
    }

    pub async fn save_settings(&self, s: &GameSettings, _ctx: &OpCtx) -> Result<()> {
        self.session.save_settings(s).await
    }

    // --- ModHub (F5, Kap. 7.7 LH) — Download durch den Server ---

    /// Download auf dem Server auslösen — **nur bei gestopptem Server** (sonst
    /// `Error::ServerRunning`); Fortschritt separat über [`Self::modhub_progress`].
    pub async fn modhub_start(&self, mod_id: u64) -> Result<()> {
        self.session.modhub_start(mod_id).await
    }

    /// Fortschritt eines laufenden ModHub-Downloads (`{downloaded, total}`).
    pub async fn modhub_progress(&self, mod_id: u64) -> Result<Progress> {
        self.session.modhub_progress(mod_id).await
    }

    /// Laufenden ModHub-Download abbrechen.
    pub async fn modhub_cancel(&self, mod_id: u64) -> Result<()> {
        self.session.modhub_cancel(mod_id).await
    }

    /// Serverseitige ModHub-Kategorieseite lesen (Kategorie-IDs Kap. 7.7 LH: 0 DLC, 1 All,
    /// 3 Update, 5 Latest, 6 Best, 7 Most Downloaded, 8 Package, 9 Official Mods,
    /// 10–13 Map, 14 Gameplay) — **nur bei gestopptem Server**.
    pub async fn modhub_category(
        &self,
        category: u8,
        page: u32,
    ) -> Result<Vec<ModhubCategoryEntry>> {
        self.session.modhub_category(category, page).await
    }

    /// Bequemlichkeit: startet, pollt bis fertig und verifiziert (Q3) den erwarteten Dateinamen
    /// in der Mod-Liste — sonst `NotProven`.
    pub async fn modhub_download<F>(
        &self,
        mod_id: u64,
        expected_file_name: &str,
        on_progress: F,
    ) -> Result<()>
    where
        F: Fn(Progress) + Send + Sync,
    {
        self.session
            .modhub_download(mod_id, expected_file_name, on_progress)
            .await
    }

    // Weitere Operationen (upload_mod, delete_mod, put_file/get_file/list_dir, Logs,
    // modhub_start/progress/cancel/download, catalog::search/details) — siehe
    // Pflichtenheft Kap. 4.3 / 4.4.
}
