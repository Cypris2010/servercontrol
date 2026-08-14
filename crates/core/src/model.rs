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
#[derive(Debug, Clone, Serialize)]
pub struct ServerMod {
    /// Kennung für `modactivate_`/`moddeactivate_` (Kap. 7.3 LH).
    pub file_name: String,
    pub display_name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub size: Option<u64>,
    pub is_dlc: bool,
    pub status: ModStatus,
    /// Zeigt das Panel neben der Version das `updateIcon.png` („An update is available for
    /// this mod.")? Live geprüft: das Icon steht in derselben Versionsspalte, die wir ohnehin
    /// parsen — verlinkt lediglich auf die ModHub-Kategorie „Update" (`category=3`), nicht auf
    /// eine konkrete `mod_id` (die holt sich die GUI bei Bedarf über die Kategorieseite, Kap. 10.1).
    pub update_available: bool,
    /// Stammt der Mod aus dem ModHub? `mods.html`-Spalte „ModHub" (`Yes`/`No`, live geprüft).
    /// Bei Herkunft von der Home-Seite (Fallback) mangels Spalte immer `false`.
    pub from_modhub: bool,
    /// Anzahl der beim letzten Serverstart geloggten Probleme mit diesem Mod (`mods.html`-
    /// Spalte „Issues", live geprüft) — z. B. zu große Texturen, zu viele Dateien. `0` = keine
    /// bekannten Probleme (oder Herkunft von der Home-Seite, die diese Spalte nicht hat).
    pub issue_count: u32,
    /// Die einzelnen Problemtexte, sofern `issue_count > 0`: kommen von der Mod-Detailseite
    /// (`mod.html?mod_index=<i>`, live geprüft) — dieselbe Seite, die auch gekürzte Felder
    /// nachliefert, daher **kein** zusätzlicher Request nur für die Issues.
    pub issues: Vec<String>,
}

/// Ein Savegame-Slot aus Sicht des Servers (`savegames.html`, Kap. 7.8 LH). Nur belegte Slots
/// erscheinen in `list_savegames`; leere Slots sind reine Ziel-Optionen beim Upload.
#[derive(Debug, Clone, Serialize)]
pub struct ServerSavegame {
    /// Slot 1..=20 — Kennung für den Download-Link `savegame<slot>` und `delete_<slot>`.
    pub slot: u8,
    pub display_name: String,
    pub map: String,
    /// In-Game-Geld, aus „500'000 $" geparst.
    pub money: u64,
    pub play_time_minutes: u32,
    pub difficulty: Difficulty,
    /// Trägt die Zeile einen Lösch-Link? **Live verifiziert:** Bei laufendem Server fehlt er nur
    /// beim **aktuell geladenen** Savegame — ein normaler Slot (auch bei laufendem Server) hat
    /// ihn weiterhin. Ebenso fehlt dieser Slot im `index_upload`-Dropdown des Upload-Formulars
    /// (Kap. 7.8 LH) — das kann man nicht überschreiben, während es gerade läuft.
    pub can_delete: bool,
}

/// Automatisch vom Server angelegtes Zeitstempel-Backup eines Slots (Kap. 7.8 LH). `timestamp`
/// ist der Formularwert-Teil nach dem Slot (z. B. `"2026-07-13_23-56"`) — zusammen mit `slot`
/// ergibt das `backup_restore=<slot>_<timestamp>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavegameBackup {
    pub slot: u8,
    pub timestamp: String,
    pub map: String,
    pub play_time_minutes: u32,
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

/// Wirtschaftliche Schwierigkeit (Server-Werte). Serialisiert als Zahl (1/2/3) — bequem für
/// die GUI-Auswahlfelder (G6), die dieselben Server-Werte als `value` der Optionen nutzen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum Difficulty {
    Easy = 1,
    Normal = 2,
    Hard = 3,
}

impl Difficulty {
    /// Aus dem Formularwert (`economicDifficulty`): "1"/"2"/"3".
    pub(crate) fn from_code(code: &str) -> Option<Self> {
        match code.trim() {
            "1" => Some(Self::Easy),
            "2" => Some(Self::Normal),
            "3" => Some(Self::Hard),
            _ => None,
        }
    }

    /// Aus dem Anzeigetext auf `savegames.html` ("Easy"/"Normal"/"Hard", Kap. 7.8 LH) —
    /// anders als [`Self::from_code`], das den numerischen Formularwert liest.
    pub(crate) fn from_label(label: &str) -> Option<Self> {
        match label.trim() {
            "Easy" => Some(Self::Easy),
            "Normal" => Some(Self::Normal),
            "Hard" => Some(Self::Hard),
            _ => None,
        }
    }
}

impl From<Difficulty> for u8 {
    fn from(d: Difficulty) -> u8 {
        d as u8
    }
}

impl TryFrom<u8> for Difficulty {
    type Error = String;
    fn try_from(v: u8) -> std::result::Result<Self, String> {
        match v {
            1 => Ok(Self::Easy),
            2 => Ok(Self::Normal),
            3 => Ok(Self::Hard),
            other => Err(format!("ungültige Schwierigkeit: {other}")),
        }
    }
}

/// „Pause wenn leer" (Server-Werte). Serialisiert als Zahl (1/2), analog [`Difficulty`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum PauseIfEmpty {
    No = 1,
    Instantly = 2,
}

impl PauseIfEmpty {
    /// Aus dem Formularwert (`pause_game_if_empty`): "1"/"2".
    pub(crate) fn from_code(code: &str) -> Option<Self> {
        match code.trim() {
            "1" => Some(Self::No),
            "2" => Some(Self::Instantly),
            _ => None,
        }
    }
}

impl From<PauseIfEmpty> for u8 {
    fn from(p: PauseIfEmpty) -> u8 {
        p as u8
    }
}

impl TryFrom<u8> for PauseIfEmpty {
    type Error = String;
    fn try_from(v: u8) -> std::result::Result<Self, String> {
        match v {
            1 => Ok(Self::No),
            2 => Ok(Self::Instantly),
            other => Err(format!("ungültiger Wert für Pause-wenn-leer: {other}")),
        }
    }
}

/// Eine Auswahlmöglichkeit eines Dropdowns: technischer `value` + Anzeigename `label`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldOption {
    pub value: String,
    pub label: String,
}

/// Verfügbare Optionen der Spieleinstellungs-Dropdowns (für die GUI-Auswahlfelder, G6).
///
/// **Live aus dem `configuration`-Formular gelesen**, damit sie zum Server passen — v. a. die
/// **Map-Liste** ist serverabhängig (Basis-Maps + jede installierte Map-Mod). `read_settings`
/// liefert die aktuelle Auswahl, diese Struktur die wählbaren Werte.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SettingsOptions {
    pub savegames: Vec<FieldOption>,
    pub maps: Vec<FieldOption>,
    pub initial_money: Vec<FieldOption>,
    pub initial_loan: Vec<FieldOption>,
    pub economic_difficulty: Vec<FieldOption>,
    pub max_player: Vec<FieldOption>,
    pub mp_language: Vec<FieldOption>,
    pub pause_game_if_empty: Vec<FieldOption>,
}

/// Eine Zeile der reinen Textanzeige der Einstellungen bei **laufendem** Server (Kap. 6.1): das
/// Panel zeigt sie dort nur als Text, nicht als Formular — `label`/`value` wie im Panel, ohne
/// die Wertebereichs-Codes aus [`GameSettings`]. `is_secret` markiert die beiden Passwort-Zeilen
/// (Administrator-/Spiel-Passwort) für eine maskierte Anzeige mit Aufdecken-Knopf in der GUI.
#[derive(Clone, Serialize)]
pub struct SettingsRow {
    pub label: String,
    pub value: String,
    pub is_secret: bool,
}

impl std::fmt::Debug for SettingsRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value: &str = if self.is_secret { "***" } else { &self.value };
        f.debug_struct("SettingsRow")
            .field("label", &self.label)
            .field("value", &value)
            .field("is_secret", &self.is_secret)
            .finish()
    }
}

/// Eine Log-Quelle des Servers: Typ (Game/Server/Webserver) mit seinen Logdateien (MZ5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogSource {
    /// Server-Code des Log-Typs (0=Server, 1=Webserver, 3=Game).
    pub log_type: u8,
    pub type_name: String,
    /// Logdateien dieses Typs, neueste zuerst.
    pub files: Vec<String>,
}

/// Übersicht der verfügbaren Logs samt aktueller `epoch` (Kennung für den Live-Log-Abruf).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogListing {
    pub epoch: u64,
    pub sources: Vec<LogSource>,
}

/// Ein inkrementell gelesener Log-Abschnitt (`tail -f` über HTTP, MZ5 / Kap. 7.4 LH).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogChunk {
    /// Neu hinzugekommener Text (aus base64 dekodiert).
    pub content: String,
    /// Nächste Byte-Position — beim nächsten Abruf als `offset` verwenden.
    pub end_offset: u64,
    /// Wird das Log gerade weitergeschrieben (Server läuft)?
    pub active: bool,
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

/// Ein Treffer der ModHub-Namenssuche (Weg B, Kap. 4.4 / 10.2 LH) — genug, um eine Trefferkarte
/// zu zeigen und „Auf Server installieren" auszulösen (`mod_id` = `startmoddownload`-Kennung).
///
/// `version`/`file_name` stehen auf der Suchergebnisseite selbst nicht drin (nur auf der
/// Detailseite des einzelnen Mods) — `catalog::search` lädt sie deshalb pro Treffer nach, damit
/// die GUI Version und „schon installiert?" genauso anzeigen kann wie beim Kategorie-Browsen
/// ([`ModhubCategoryEntry`], die der Server direkt mitliefert).
#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntry {
    pub mod_id: u64,
    pub name: String,
    pub author: Option<String>,
    pub rating: Option<f32>,
    pub thumb_url: Option<String>,
    pub version: Option<String>,
    pub file_name: Option<String>,
}

/// Ein Eintrag der **serverseitigen** ModHub-Kategorieseite (Pflichtenheft 10.1) — der Server
/// selbst kennt Version/Autor/Dateiname/Größe, aber keine Bewertung/kein Vorschaubild (die
/// liefert nur die öffentliche Website, [`CatalogEntry`]). Nur bei gestopptem Server lesbar
/// (Kap. 7.7 LH). `mod_id` ist dieselbe Kennung wie bei [`CatalogEntry`] (`startmoddownload`).
#[derive(Debug, Clone, Serialize)]
pub struct ModhubCategoryEntry {
    pub mod_id: u64,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub file_name: String,
    pub size: Option<u64>,
}

/// Detailinfos eines ModHub-Eintrags (Kap. 4.4 / 10.2 LH) — für eine optionale Detailansicht.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogDetails {
    pub mod_id: u64,
    pub name: String,
    pub author: Option<String>,
    pub version: Option<String>,
    pub file_name: Option<String>,
    pub description: Option<String>,
}
