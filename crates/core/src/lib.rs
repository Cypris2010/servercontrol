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

// Interne Bausteine (Projektstruktur, Pflichtenheft Kap. 3) — noch umzusetzen.
mod control;
mod logs;
mod modhub;
mod mods;
mod session;
mod settings;
mod verify;

pub use error::Error;
pub use model::{
    Difficulty, GameSettings, ModStatus, OpCtx, PauseIfEmpty, Progress, ServerMod, ServerState,
};
pub use profile::{FileAccess, FileProtocol, ServerProfile};
pub use secret::Secret;

/// Bequemer Ergebnistyp der Bibliothek.
pub type Result<T> = std::result::Result<T, Error>;

/// Verbindung/Sitzung zu einem FS25-Dedicated-Server (Pflichtenheft 4.3).
///
/// Alle eingreifenden Operationen **verifizieren selbst** (Kap. 9) und geben erst bei
/// nachgewiesenem Ergebnis `Ok` zurück (`Error::NotProven` bei Zeitüberschreitung).
pub struct ServerControl {
    // Cookie-Sitzung, HTTP-Client, Profil — folgen bei der Umsetzung.
    _private: (),
}

impl ServerControl {
    /// Anmelden, Cookie-Sitzung aufbauen, Version erkennen (F2, Kap. 7.3 LH).
    pub async fn connect(_profile: &ServerProfile, _ctx: &OpCtx) -> Result<Self> {
        todo!("Umsetzung: session::connect")
    }

    /// Aktueller Laufzeitzustand (online/offline + Spielversion).
    pub async fn state(&self) -> Result<ServerState> {
        todo!()
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
