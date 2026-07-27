//! Tauri-App: dünne Command-Schicht über `servercontrol-core`. Die Bibliothek ist der einzige
//! Ort mit Logik; hier wird nur die Sitzung im App-State gehalten und für das Frontend
//! serialisiert.

use serde::{Deserialize, Serialize};
use servercontrol_core::{
    Difficulty, GameSettings, ModStatus, OpCtx, PauseIfEmpty, ProfileId, ServerControl, ServerMod,
    ServerProfile, ServerState, SettingsOptions, SettingsRow,
};
use tokio::sync::Mutex;

/// App-Zustand: die aktive Sitzung (eine je verbundenem Server) samt zugehöriger Profil-ID.
/// `None` = nicht verbunden.
#[derive(Default)]
struct AppState {
    sc: Mutex<Option<(ProfileId, ServerControl)>>,
}

/// Kompakte Übersicht für die Startseite (Zustand + Mod-Zahlen).
#[derive(Serialize)]
struct Overview {
    online: bool,
    version: Option<String>,
    mod_total: usize,
    mod_active: usize,
    mod_inactive: usize,
    mod_dlc: usize,
}

async fn build_overview(sc: &ServerControl) -> Result<Overview, String> {
    let state = sc.state().await.map_err(|e| e.to_string())?;
    let mods = sc.list_mods().await.map_err(|e| e.to_string())?;
    let (online, version) = match state {
        ServerState::Online { version } => (true, version),
        ServerState::Offline => (false, None),
    };
    let active = mods
        .iter()
        .filter(|m| m.status == ModStatus::Active)
        .count();
    let dlc = mods.iter().filter(|m| m.is_dlc).count();
    Ok(Overview {
        online,
        version,
        mod_total: mods.len(),
        mod_active: active,
        mod_inactive: mods.len() - active,
        mod_dlc: dlc,
    })
}

// --- G1: Profilverwaltung (Pflichtenheft 7.4) ---

/// Profil samt Info, ob im Credential-Store bereits ein Passwort liegt (Platzhaltertext in
/// der GUI) — ohne das Passwort selbst zu übertragen.
#[derive(Serialize)]
struct ProfileDto {
    #[serde(flatten)]
    profile: ServerProfile,
    has_password: bool,
    has_ftp_password: bool,
}

fn to_dto(p: ServerProfile) -> ProfileDto {
    let has_password = servercontrol_core::has_password(&p.credential_key);
    let has_ftp_password = p
        .file_access
        .as_ref()
        .map(|fa| servercontrol_core::has_password(&fa.credential_key))
        .unwrap_or(false);
    ProfileDto {
        profile: p,
        has_password,
        has_ftp_password,
    }
}

/// Alle Profile lesen (kein Hintergrund-Check — reines Lesen der Profildatei, Kap. 7.4).
#[tauri::command]
async fn list_profiles() -> Result<Vec<ProfileDto>, String> {
    let profiles = servercontrol_core::load_profiles().map_err(|e| e.to_string())?;
    Ok(profiles.into_iter().map(to_dto).collect())
}

/// Profil anlegen/bearbeiten. `web_password`/`ftp_password` sind nur bei Änderung gesetzt
/// (leer = vorhandenes Passwort im Credential-Store behalten). `credential_key`-Felder werden
/// hier — nicht vom Frontend — aus der stabilen `id` abgeleitet (Kap. 8.4).
#[tauri::command]
async fn save_profile(
    mut profile: ServerProfile,
    web_password: Option<String>,
    ftp_password: Option<String>,
) -> Result<ProfileDto, String> {
    let mut profiles = servercontrol_core::load_profiles().map_err(|e| e.to_string())?;
    let is_new = !profiles.iter().any(|p| p.id == profile.id);

    profile.credential_key = ServerProfile::web_credential_key(profile.id);
    if let Some(fa) = profile.file_access.as_mut() {
        fa.credential_key = ServerProfile::ftp_credential_key(profile.id);
    }

    let web_password = web_password.filter(|p| !p.is_empty());
    if is_new && web_password.is_none() {
        return Err("Web-Passwort erforderlich".to_string());
    }
    if let Some(pw) = web_password {
        servercontrol_core::store_password(&profile.credential_key, pw.into())
            .map_err(|e| e.to_string())?;
    }

    let ftp_password = ftp_password.filter(|p| !p.is_empty());
    if let Some(fa) = &profile.file_access {
        if is_new && ftp_password.is_none() {
            return Err("FTP/SFTP-Passwort erforderlich".to_string());
        }
        if let Some(pw) = ftp_password {
            servercontrol_core::store_password(&fa.credential_key, pw.into())
                .map_err(|e| e.to_string())?;
        }
    }

    if is_new {
        profiles.push(profile.clone());
    } else {
        let idx = profiles.iter().position(|p| p.id == profile.id).unwrap();
        profiles[idx] = profile.clone();
    }
    servercontrol_core::save_profiles(&profiles).map_err(|e| e.to_string())?;
    Ok(to_dto(profile))
}

/// Profil löschen — inklusive der zugehörigen Credential-Store-Einträge (Kap. 8.4).
#[tauri::command]
async fn delete_profile(state: tauri::State<'_, AppState>, id: ProfileId) -> Result<(), String> {
    let mut profiles = servercontrol_core::load_profiles().map_err(|e| e.to_string())?;
    if let Some(idx) = profiles.iter().position(|p| p.id == id) {
        let removed = profiles.remove(idx);
        servercontrol_core::delete_profile_credentials(&removed).map_err(|e| e.to_string())?;
    }
    servercontrol_core::save_profiles(&profiles).map_err(|e| e.to_string())?;

    // War das gelöschte Profil verbunden, Sitzung sauber trennen.
    let mut guard = state.sc.lock().await;
    if guard.as_ref().is_some_and(|(sid, _)| *sid == id) {
        if let Some((_, sc)) = guard.take() {
            let _ = sc.logout().await;
        }
    }
    Ok(())
}

/// Profil duplizieren (eigene ID, **kein** übernommenes Passwort — eigener Credential-Eintrag,
/// muss beim Bearbeiten neu gesetzt werden).
#[tauri::command]
async fn duplicate_profile(id: ProfileId) -> Result<ProfileDto, String> {
    let mut profiles = servercontrol_core::load_profiles().map_err(|e| e.to_string())?;
    let src = profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or("Profil nicht gefunden")?
        .clone();

    let new_id = ProfileId::new_v4();
    let mut copy = src;
    copy.id = new_id;
    copy.name = format!("{} (Kopie)", copy.name);
    copy.credential_key = ServerProfile::web_credential_key(new_id);
    if let Some(fa) = copy.file_access.as_mut() {
        fa.credential_key = ServerProfile::ftp_credential_key(new_id);
    }

    profiles.push(copy.clone());
    servercontrol_core::save_profiles(&profiles).map_err(|e| e.to_string())?;
    Ok(to_dto(copy))
}

/// Mit einem Profil verbinden (F2). Trennt zuvor eine ggf. laufende Sitzung sauber ab —
/// keine Vermischung von Cookies zweier Server (Kap. 7.4).
#[tauri::command]
async fn connect_profile(
    state: tauri::State<'_, AppState>,
    id: ProfileId,
) -> Result<Overview, String> {
    let profiles = servercontrol_core::load_profiles().map_err(|e| e.to_string())?;
    let profile = profiles
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("Profil nicht gefunden")?;

    if let Some((_, old)) = state.sc.lock().await.take() {
        let _ = old.logout().await;
    }
    let sc = ServerControl::connect(&profile, &OpCtx)
        .await
        .map_err(|e| e.to_string())?;
    let overview = build_overview(&sc).await?;
    *state.sc.lock().await = Some((id, sc));
    Ok(overview)
}

/// Aktuell verbundenes Profil (falls vorhanden) — für die Statusleiste nach einem
/// Ansichtswechsel, ohne eigenen clientseitigen Zustand doppelt zu führen.
#[tauri::command]
async fn active_profile_id(state: tauri::State<'_, AppState>) -> Result<Option<ProfileId>, String> {
    Ok(state.sc.lock().await.as_ref().map(|(id, _)| *id))
}

/// Übersicht neu lesen (Zustand + Mods).
#[tauri::command]
async fn overview(state: tauri::State<'_, AppState>) -> Result<Overview, String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    build_overview(sc).await
}

/// Verbindung trennen (Sitzung abmelden und verwerfen).
#[tauri::command]
async fn disconnect(state: tauri::State<'_, AppState>) -> Result<(), String> {
    if let Some((_, sc)) = state.sc.lock().await.take() {
        let _ = sc.logout().await;
    }
    Ok(())
}

/// G2 (7.5): Zustand + Modliste in einem Aufruf — der Zustand entscheidet die Sperr-Logik (7.2).
#[derive(Serialize)]
struct ModsView {
    online: bool,
    mods: Vec<ServerMod>,
}

#[tauri::command]
async fn mods_view(state: tauri::State<'_, AppState>) -> Result<ModsView, String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    let online = matches!(
        sc.state().await.map_err(|e| e.to_string())?,
        ServerState::Online { .. }
    );
    let mods = sc.list_mods().await.map_err(|e| e.to_string())?;
    Ok(ModsView { online, mods })
}

/// Stapel-Aktivierung/-Deaktivierung (7.5) — nur bei gestopptem Server möglich
/// (`Error::ServerRunning` sonst).
#[tauri::command]
async fn set_active(
    state: tauri::State<'_, AppState>,
    activate: Vec<String>,
    deactivate: Vec<String>,
) -> Result<(), String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    sc.set_active(&activate, &deactivate)
        .await
        .map_err(|e| e.to_string())
}

/// Mod-Datei löschen (7.5, destruktiv) — nur bei gestopptem Server möglich.
#[tauri::command]
async fn delete_mod(state: tauri::State<'_, AppState>, file_name: String) -> Result<(), String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    sc.delete_mod(&file_name).await.map_err(|e| e.to_string())
}

// --- G4: Serversteuerung im Kopf (Pflichtenheft 7.7) ---
//
// start/stop/restart verifizieren intern selbst (Q3, Kap. 9) und geben erst bei
// nachgewiesenem Ergebnis zurück — die Sperre der Buttons während des Aufrufs hält die
// GUI so lange busy, wie die Bibliothek für den Nachweis braucht (bis zu 5 Minuten beim Start).

#[tauri::command]
async fn start_server(state: tauri::State<'_, AppState>) -> Result<Overview, String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    sc.start(&OpCtx).await.map_err(|e| e.to_string())?;
    build_overview(sc).await
}

#[tauri::command]
async fn stop_server(state: tauri::State<'_, AppState>) -> Result<Overview, String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    sc.stop(&OpCtx).await.map_err(|e| e.to_string())?;
    build_overview(sc).await
}

#[tauri::command]
async fn restart_server(state: tauri::State<'_, AppState>) -> Result<Overview, String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    sc.restart(&OpCtx).await.map_err(|e| e.to_string())?;
    build_overview(sc).await
}

// --- G6: Spieleinstellungen (Pflichtenheft 7.9) ---
//
// `GameSettings` trägt bewusst kein `Serialize` — das `Secret`-Feld soll sich nicht aus
// Versehen irgendwo im Code (Log, Fehlermeldung, künftige Serialisierung) mitziehen lassen.
// Hier an der einen Stelle, wo die GUI die Klartext-Passwörter zum Anzeigen/Bearbeiten
// braucht (7.9: „Passwörter maskiert mit Anzeigen-Knopf"), werden sie **explizit** entpackt.

#[derive(Serialize, Deserialize)]
struct GameSettingsDto {
    game_name: String,
    admin_password: String,
    game_password: String,
    savegame: u8,
    map_start: String,
    initial_money: u32,
    initial_loan: u32,
    economic_difficulty: u8,
    server_port: u16,
    max_player: u8,
    mp_language: String,
    auto_save_interval: u32,
    stats_interval: u32,
    pause_game_if_empty: u8,
    crossplay_allowed: bool,
}

impl From<GameSettings> for GameSettingsDto {
    fn from(s: GameSettings) -> Self {
        Self {
            game_name: s.game_name,
            admin_password: s.admin_password.expose().to_string(),
            game_password: s.game_password.expose().to_string(),
            savegame: s.savegame,
            map_start: s.map_start,
            initial_money: s.initial_money,
            initial_loan: s.initial_loan,
            economic_difficulty: s.economic_difficulty.into(),
            server_port: s.server_port,
            max_player: s.max_player,
            mp_language: s.mp_language,
            auto_save_interval: s.auto_save_interval,
            stats_interval: s.stats_interval,
            pause_game_if_empty: s.pause_game_if_empty.into(),
            crossplay_allowed: s.crossplay_allowed,
        }
    }
}

impl TryFrom<GameSettingsDto> for GameSettings {
    type Error = String;
    fn try_from(d: GameSettingsDto) -> Result<Self, String> {
        Ok(Self {
            game_name: d.game_name,
            admin_password: d.admin_password.into(),
            game_password: d.game_password.into(),
            savegame: d.savegame,
            map_start: d.map_start,
            initial_money: d.initial_money,
            initial_loan: d.initial_loan,
            economic_difficulty: Difficulty::try_from(d.economic_difficulty)?,
            server_port: d.server_port,
            max_player: d.max_player,
            mp_language: d.mp_language,
            auto_save_interval: d.auto_save_interval,
            stats_interval: d.stats_interval,
            pause_game_if_empty: PauseIfEmpty::try_from(d.pause_game_if_empty)?,
            crossplay_allowed: d.crossplay_allowed,
        })
    }
}

/// Einstellungen + Dropdown-Optionen in einem Aufruf. Beide sind am laufenden Server nicht als
/// Formular lesbar (das Panel zeigt online nur Text) — dann `settings`/`options` `None` und
/// stattdessen `summary` (Label/Wert-Textanzeige, 7.9).
#[derive(Serialize)]
struct SettingsView {
    online: bool,
    settings: Option<GameSettingsDto>,
    options: Option<SettingsOptions>,
    summary: Option<Vec<SettingsRow>>,
}

#[tauri::command]
async fn settings_view(state: tauri::State<'_, AppState>) -> Result<SettingsView, String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    let online = matches!(
        sc.state().await.map_err(|e| e.to_string())?,
        ServerState::Online { .. }
    );
    if online {
        let summary = sc
            .read_settings_summary()
            .await
            .map_err(|e| e.to_string())?;
        return Ok(SettingsView {
            online,
            settings: None,
            options: None,
            summary: Some(summary),
        });
    }
    let settings = sc.read_settings().await.map_err(|e| e.to_string())?.into();
    let options = sc
        .read_settings_options()
        .await
        .map_err(|e| e.to_string())?;
    Ok(SettingsView {
        online,
        settings: Some(settings),
        options: Some(options),
        summary: None,
    })
}

/// Speichern — nur bei gestopptem Server möglich (sonst `Error::ServerRunning`, 7.2). Die
/// Bibliothek fährt den vollen Formular-Umlauf und verifiziert das Ergebnis selbst (Q3).
#[tauri::command]
async fn save_settings(
    state: tauri::State<'_, AppState>,
    settings: GameSettingsDto,
) -> Result<(), String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    let settings: GameSettings = settings.try_into()?;
    sc.save_settings(&settings, &OpCtx)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_profiles,
            save_profile,
            delete_profile,
            duplicate_profile,
            connect_profile,
            active_profile_id,
            overview,
            disconnect,
            mods_view,
            set_active,
            delete_mod,
            start_server,
            stop_server,
            restart_server,
            settings_view,
            save_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
