//! Tauri-App: dünne Command-Schicht über `servercontrol-core`. Die Bibliothek ist der einzige
//! Ort mit Logik; hier wird nur die Sitzung im App-State gehalten und für das Frontend
//! serialisiert.

use serde::Serialize;
use servercontrol_core::{ModStatus, OpCtx, ServerControl, ServerProfile, ServerState};
use tokio::sync::Mutex;

/// App-Zustand: die aktive Sitzung (eine je verbundenem Server). `None` = nicht verbunden.
#[derive(Default)]
struct AppState {
    sc: Mutex<Option<ServerControl>>,
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
    let active = mods.iter().filter(|m| m.status == ModStatus::Active).count();
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

/// Verbinden: Profil bauen (Passwort kommt aus dem OS-Credential-Store), Sitzung halten,
/// Übersicht zurückgeben. `credential_key` ist in dieser frühen Version fix (`livetest/web`);
/// die echte Profilverwaltung (G1) folgt.
#[tauri::command]
async fn connect(
    state: tauri::State<'_, AppState>,
    url: String,
    username: String,
) -> Result<Overview, String> {
    let profile = ServerProfile {
        name: "GUI".to_string(),
        base_url: url.parse().map_err(|_| "Ungültige Adresse".to_string())?,
        username,
        credential_key: "livetest/web".to_string(),
        accept_invalid_cert: false,
        file_access: None,
    };
    let sc = ServerControl::connect(&profile, &OpCtx)
        .await
        .map_err(|e| e.to_string())?;
    let overview = build_overview(&sc).await?;
    *state.sc.lock().await = Some(sc);
    Ok(overview)
}

/// Übersicht neu lesen (Zustand + Mods).
#[tauri::command]
async fn overview(state: tauri::State<'_, AppState>) -> Result<Overview, String> {
    let guard = state.sc.lock().await;
    let sc = guard.as_ref().ok_or("Nicht verbunden")?;
    build_overview(sc).await
}

/// Verbindung trennen (Sitzung abmelden und verwerfen).
#[tauri::command]
async fn disconnect(state: tauri::State<'_, AppState>) -> Result<(), String> {
    if let Some(sc) = state.sc.lock().await.take() {
        let _ = sc.logout().await;
    }
    Ok(())
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
        .invoke_handler(tauri::generate_handler![connect, overview, disconnect])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
