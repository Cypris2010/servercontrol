use crate::secret::Secret;
use serde::{Deserialize, Serialize};

/// Laufzeitzustand des Spielservers (Pflichtenheft 4.2 / 9.1).
///
/// Erkennung über `div.status-indicator` (`online`/`offline`). `version` ist die
/// **Spielversion** aus dem Statistik-Block (nur online verfügbar), **nicht** der
/// Web-Interface-Build aus dem Footer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerState {
    Online { version: Option<String> },
    Offline,
}

/// Status eines Mods aus Sicht des Servers (Kap. 7.3 / 10.5 LH).
///
/// `Orphan` = Registry-Eintrag ohne vorhandene Datei (Karteileiche).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModStatus {
    Active,
    Inactive,
    Orphan,
}

/// Ein Mod im Serverbestand.
#[derive(Debug, Clone)]
pub struct ServerMod {
    /// Kennung für `modactivate_`/`moddeactivate_` (Kap. 7.3 LH).
    pub file_name: String,
    pub display_name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub size: Option<u64>,
    pub is_dlc: bool,
    pub status: ModStatus,
}

/// Spiel-Einstellungen = Felder des `configuration`-Formulars (Kap. 6.1),
/// Wertebereiche am Server verifiziert.
#[derive(Debug, Clone)]
pub struct GameSettings {
    pub game_name: String,
    pub admin_password: Secret,
    pub game_password: Secret,
    /// Savegame-Slot 1..=20.
    pub savegame: u8,
    // --- nur bei leerem Savegame editierbar (checkSavegame, Kap. 6) ---
    pub map_start: String,
    pub initial_money: u32,
    pub initial_loan: u32,
    pub economic_difficulty: Difficulty,
    // --- immer editierbar ---
    pub server_port: u16,
    /// Max. Spieler 2..=16.
    pub max_player: u8,
    /// Sprachcode ("en", "de", …).
    pub mp_language: String,
    /// Auto-Save-Intervall in Minuten.
    pub auto_save_interval: u32,
    /// „Web API Interval" in Sekunden (großer Wert = Feed praktisch aus, Kap. 7.5 LH).
    pub stats_interval: u32,
    pub pause_game_if_empty: PauseIfEmpty,
    pub crossplay_allowed: bool,
}

/// Wirtschaftliche Schwierigkeit (Server-Werte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Easy = 1,
    Normal = 2,
    Hard = 3,
}

/// „Pause wenn leer" (Server-Werte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseIfEmpty {
    No = 1,
    Instantly = 2,
}

/// Laufzeit-Kontext für lange Operationen: Abbruch + Fortschritt (Pflichtenheft 4.2).
///
/// Platzhalter — wird um `CancellationToken` und eine `ProgressSink` erweitert.
#[derive(Default)]
pub struct OpCtx;

/// Fortschritt eines Downloads/Uploads (`{downloaded, total}`, Kap. 7.7 LH).
#[derive(Debug, Clone, Copy)]
pub struct Progress {
    pub done: u64,
    pub total: Option<u64>,
}
