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
    Difficulty, GameSettings, ModStatus, OpCtx, PauseIfEmpty, Progress, ServerMod, ServerState,
};
pub use profile::{FileAccess, FileProtocol, ServerProfile};
pub use secret::Secret;

/// Bequemer Ergebnistyp der Bibliothek.
pub type Result<T> = std::result::Result<T, Error>;

/// Passwort im OS-Credential-Store hinterlegen (Q1). Von CLI/GUI beim Einrichten eines
/// Profils genutzt; die Bibliothek liest es beim [`ServerControl::connect`] von dort und
/// hält es nie im Repo oder in Logs.
pub fn store_password(credential_key: &str, password: Secret) -> Result<()> {
    credentials::store(credential_key, &password)
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

    // --- Mods (F3/F4) ---

    pub async fn list_mods(&self) -> Result<Vec<ServerMod>> {
        todo!()
    }

    /// Nur bei **gestopptem** Server möglich (sonst `Error::ServerRunning`).
    pub async fn set_active(&self, _activate: &[String], _deactivate: &[String]) -> Result<()> {
        todo!()
    }

    // --- Steuerung (F6) — Voll-Formular-Umlauf, Ergebnis am Zustand belegt ---

    pub async fn start(&self, _ctx: &OpCtx) -> Result<()> {
        todo!()
    }
    pub async fn stop(&self, _ctx: &OpCtx) -> Result<()> {
        todo!()
    }
    pub async fn restart(&self, _ctx: &OpCtx) -> Result<()> {
        todo!()
    }

    // --- Spieleinstellungen (Kann/G6) ---

    pub async fn read_settings(&self) -> Result<GameSettings> {
        todo!()
    }
    pub async fn save_settings(&self, _s: &GameSettings, _ctx: &OpCtx) -> Result<()> {
        todo!()
    }

    // Weitere Operationen (upload_mod, delete_mod, put_file/get_file/list_dir, Logs,
    // modhub_start/progress/cancel/download, catalog::search/details) — siehe
    // Pflichtenheft Kap. 4.3 / 4.4.
}
