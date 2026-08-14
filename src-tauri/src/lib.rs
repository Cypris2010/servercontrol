//! Tauri-App: dünne Command-Schicht über `servercontrol-core`. Die Bibliothek ist der einzige
//! Ort mit Logik; hier wird nur die Sitzung im App-State gehalten und für das Frontend
//! serialisiert.

use serde::{Deserialize, Serialize};
use servercontrol_core::{
    catalog, CatalogEntry, Difficulty, FieldOption, GameSettings, ModStatus, OpCtx, PauseIfEmpty,
    ProfileId, SavegameBackup, ServerControl, ServerMod, ServerProfile, ServerSavegame,
    ServerState, SettingsOptions, SettingsRow,
};
use tauri::Emitter;
use tokio::sync::Mutex;

/// App-Zustand: die aktive Sitzung (eine je verbundenem Server) samt zugehöriger Profil-ID.
/// `None` = nicht verbunden.
#[derive(Default)]
struct AppState {
    sc: Mutex<Option<(ProfileId, ServerControl)>>,
}

/// Kompakte Übersicht für die Startseite (Zustand + Mod-/Savegame-Zahlen).
#[derive(Serialize)]
struct Overview {
    online: bool,
    version: Option<String>,
    mod_total: usize,
    mod_active: usize,
    mod_inactive: usize,
    mod_dlc: usize,
    savegame_total: usize,
    /// Map des aktuell geladenen Savegames, falls eines läuft — erkannt an `can_delete = false`
    /// (Kap. 7.8 LH: nur das gerade geladene Savegame hat live verifiziert keinen Lösch-Link).
    savegame_active_map: Option<String>,
}

async fn build_overview(sc: &ServerControl) -> Result<Overview, String> {
    let state = sc.state().await.map_err(|e| e.to_string())?;
    let mods = sc.list_mods().await.map_err(|e| e.to_string())?;
    let savegames = sc.list_savegames().await.map_err(|e| e.to_string())?;
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
        savegame_total: savegames.len(),
        savegame_active_map: savegames
            .iter()
            .find(|s| !s.can_delete)
            .map(|s| s.map.clone()),
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

/// `progress`-Event-Payload (Kap. 7.3) für den laufenden Upload.
#[derive(Clone, Serialize)]
struct UploadProgress {
    done: u64,
    total: Option<u64>,
}

/// Gemeinsamer Kern für `upload_mod`/`overwrite_mod`: hochladen und dabei den Fortschritt über
/// das `progress`-Event melden, gedrosselt auf ~1 % Schritte (bzw. min. 256 KB) — sonst würden
/// Zehntausende Einzel-Events pro Datei die IPC-Brücke fluten.
async fn run_upload_with_progress(
    app: &tauri::AppHandle,
    sc: &ServerControl,
    path: &str,
) -> Result<(), String> {
    let last_emitted = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let app = app.clone();
    sc.upload_mod_with_progress(std::path::Path::new(path), move |p| {
        let total = p.total.unwrap_or(0);
        let step = (total / 100).max(262_144);
        let prev = last_emitted.load(std::sync::atomic::Ordering::Relaxed);
        if p.done.saturating_sub(prev) >= step || Some(p.done) == p.total {
            last_emitted.store(p.done, std::sync::atomic::Ordering::Relaxed);
            let _ = app.emit(
                "progress",
                UploadProgress {
                    done: p.done,
                    total: p.total,
                },
            );
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Mod über das Web-Formular hochladen (G3, 7.6) — bis 1,71 GB, sonst `Error::NoFileAccess`
/// (FTP/SFTP-Weg ist noch nicht umgesetzt). `path` kommt vom nativen Datei-Dialog (Frontend),
/// die Bibliothek liest die Datei selbst ein und verifiziert danach die Modliste (Q3).
///
/// **Überschreibt eine bereits vorhandene Datei gleichen Namens nicht** (live gegen den Server
/// verifiziert) — dafür gibt es [`overwrite_mod`]. Die GUI klärt das vorher über [`plan_uploads`].
#[tauri::command]
async fn upload_mod(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    run_upload_with_progress(&app, sc, &path).await
}

/// War `file_name` vor einem Update aktiv? Ein Update (Löschen+Neu-Hochladen wie ein frischer
/// ModHub-Download) legt die Datei neu an — das Panel stuft sie dabei immer als **inaktiv**
/// ein, selbst wenn die alte Version aktiv war (live beobachtet). Damit ein Update das nicht
/// unbemerkt zurücksetzt, merken wir uns den Stand vorher und reaktivieren ihn danach über
/// [`restore_active_if_needed`] — der Dateiname bleibt über das Update hinweg derselbe stabile
/// Schlüssel (Kap. 7.3 LH).
async fn was_active(sc: &ServerControl, file_name: &str) -> Result<bool, String> {
    Ok(sc
        .list_mods()
        .await
        .map_err(|e| e.to_string())?
        .iter()
        .any(|m| m.file_name == file_name && m.status == ModStatus::Active))
}

/// Nach einem Update den vorherigen Aktivierungsstatus wiederherstellen, falls nötig — nur bei
/// gestopptem Server aufrufbar, was Löschen/ModHub-Download hier ohnehin schon voraussetzen.
async fn restore_active_if_needed(
    sc: &ServerControl,
    file_name: &str,
    was_active: bool,
) -> Result<(), String> {
    if was_active {
        sc.set_active(&[file_name.to_string()], &[])
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Vorhandene Mod-Datei ersetzen: da reines Hochladen nicht überschreibt (s. o.), erst löschen
/// (nur bei gestopptem Server, `Error::ServerRunning` sonst — wie `delete_mod`) und dann neu
/// hochladen, mit Fortschritt wie [`upload_mod`]. War der Mod vorher aktiv, wird er danach
/// automatisch wieder aktiviert (s. [`was_active`]).
#[tauri::command]
async fn overwrite_mod(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
    file_name: String,
) -> Result<(), String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    let active_before = was_active(sc, &file_name).await?;
    sc.delete_mod(&file_name).await.map_err(|e| e.to_string())?;
    run_upload_with_progress(&app, sc, &path).await?;
    restore_active_if_needed(sc, &file_name, active_before).await
}

/// Ein einzelner Eintrag im Upload-Plan (G3): eigene Version gegen den Serverstand abgeglichen,
/// damit die GUI vor dem eigentlichen Hochladen entscheiden lässt (7.6).
#[derive(Serialize)]
struct UploadPlanItem {
    path: String,
    file_name: String,
    is_fs25_mod: bool,
    local_version: Option<String>,
    exists_on_server: bool,
    server_version: Option<String>,
    server_status: Option<ModStatus>,
}

/// Für jede gewählte Datei: lokal prüfen, ob überhaupt ein FS25-Mod drinsteckt und welche
/// Version er deklariert (`modDesc.xml`, kein Upload), und mit der aktuellen Modliste des
/// Servers abgleichen (Abgleich über den Dateinamen — so identifiziert der Server Mods auch
/// sonst). Reine Lesevorgänge, kein Schreibzugriff.
#[tauri::command]
async fn plan_uploads(
    state: tauri::State<'_, AppState>,
    paths: Vec<String>,
) -> Result<Vec<UploadPlanItem>, String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    let server_mods = sc.list_mods().await.map_err(|e| e.to_string())?;

    let mut items = Vec::with_capacity(paths.len());
    for path in paths {
        let file_name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let read_path = path.clone();
        let info = tokio::task::spawn_blocking(move || {
            servercontrol_core::inspect_local_mod(std::path::Path::new(&read_path))
        })
        .await
        .unwrap_or_default();
        let existing = server_mods.iter().find(|m| m.file_name == file_name);
        items.push(UploadPlanItem {
            path,
            file_name,
            is_fs25_mod: info.is_fs25_mod,
            local_version: info.version,
            exists_on_server: existing.is_some(),
            server_version: existing.and_then(|m| m.version.clone()),
            server_status: existing.map(|m| m.status),
        });
    }
    Ok(items)
}

/// Alle FS25-Mod-Dateien (`.zip`/`.dlc` mit `modDesc.xml`) direkt in einem Ordner auflisten
/// (nicht rekursiv, wie der Mods-Ordner des Servers selbst) — für die Ordnerauswahl beim
/// Bereitstellen (G3). Andere Zip-Dateien im Ordner werden stillschweigend übersprungen.
#[tauri::command]
async fn list_mod_files(dir: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if (ext == "zip" || ext == "dlc")
                && servercontrol_core::inspect_local_mod(&path).is_fs25_mod
            {
                files.push(path.to_string_lossy().to_string());
            }
        }
        files.sort();
        Ok(files)
    })
    .await
    .map_err(|e| e.to_string())?
}

// --- G3b: ModHub-Suche (Kann-Ziel, Pflichtenheft 4.4 / 7.7 LH) ---
//
// Die Suche selbst läuft gegen die öffentliche ModHub-Website, ohne Server-Sitzung (`catalog`
// braucht kein `connect`). Der eigentliche Download läuft dagegen **durch den Server** — dafür
// ist eine verbundene Sitzung nötig, und der Server erzwingt „nur bei gestopptem Server".

/// Namenssuche auf der öffentlichen ModHub-Website — unabhängig von einer Server-Sitzung.
#[tauri::command]
async fn modhub_search(query: String) -> Result<Vec<CatalogEntry>, String> {
    catalog::search(&query).await.map_err(|e| e.to_string())
}

/// `progress`-Event-Payload für einen laufenden ModHub-Download — je `mod_id` eine eigene
/// Fortschrittszeile in der GUI, analog zur Datei-Upload-Warteschlange.
#[derive(Clone, Serialize)]
struct ModhubProgress {
    mod_id: u64,
    done: u64,
    total: Option<u64>,
}

/// Einen ModHub-Mod auf dem Server installieren, dann bis zum Ende pollen — nur bei gestopptem
/// Server (sonst `Error::ServerRunning`, wie Mod-Löschen/-Umschalten). Für die Q3-Verifikation
/// (steht der Mod danach in der Liste?) braucht es den erwarteten Dateinamen: kommt er aus der
/// Server-Kategorieseite (`modhub_browse_category`), kennt die GUI ihn schon und übergibt ihn
/// direkt — sonst (Namenssuche über die öffentliche Website) wird er hier über die Detailseite
/// nachgeholt. War eine gleichnamige, bereits vorhandene Datei (Update-Fall) aktiv, wird sie
/// danach automatisch wieder aktiviert (s. [`was_active`]) — ein Neuanlegen stuft das Panel
/// sonst immer als inaktiv ein.
#[tauri::command]
async fn modhub_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mod_id: u64,
    file_name: Option<String>,
) -> Result<(), String> {
    let file_name = match file_name {
        Some(f) => f,
        None => catalog::details(mod_id)
            .await
            .map_err(|e| e.to_string())?
            .file_name
            .ok_or("ModHub-Detailseite ohne Dateinamen")?,
    };

    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    let active_before = was_active(sc, &file_name).await?;

    let last_emitted = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let app_handle = app.clone();
    sc.modhub_download(mod_id, &file_name, move |p| {
        let total = p.total.unwrap_or(0);
        let step = (total / 100).max(262_144);
        let prev = last_emitted.load(std::sync::atomic::Ordering::Relaxed);
        if p.done.saturating_sub(prev) >= step || Some(p.done) == p.total {
            last_emitted.store(p.done, std::sync::atomic::Ordering::Relaxed);
            let _ = app_handle.emit(
                "modhub-progress",
                ModhubProgress {
                    mod_id,
                    done: p.done,
                    total: p.total,
                },
            );
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    restore_active_if_needed(sc, &file_name, active_before).await
}

/// Laufenden ModHub-Download abbrechen.
#[tauri::command]
async fn modhub_cancel(state: tauri::State<'_, AppState>, mod_id: u64) -> Result<(), String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    sc.modhub_cancel(mod_id).await.map_err(|e| e.to_string())
}

/// Serverseitige ModHub-Kategorieseite lesen (Kap. 7.7/10.1 LH) — nur bei gestopptem Server
/// (sonst `Error::ServerRunning`). Liefert bereits den Dateinamen je Eintrag, damit
/// `modhub_install` danach ohne einen weiteren Abruf der öffentlichen Website auskommt.
#[tauri::command]
async fn modhub_browse_category(
    state: tauri::State<'_, AppState>,
    category: u8,
    page: u32,
) -> Result<Vec<servercontrol_core::ModhubCategoryEntry>, String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    sc.modhub_category(category, page)
        .await
        .map_err(|e| e.to_string())
}

// --- G7: Savegames (Kann-Ziel, Pflichtenheft 7.9a / Kap. 7.8 LH) ---
//
// Anders als G2/G6 keine Sperre durch den Serverzustand — Upload/Löschen/Restore funktionieren
// bei laufendem wie bei gestopptem Server (live verifiziert). `online` wird trotzdem mitgegeben,
// falls die GUI es für die Anzeige braucht (z. B. Serversteuerung im Kopf).

#[derive(Serialize)]
struct SavegamesView {
    online: bool,
    savegames: Vec<ServerSavegame>,
    /// Die echten `index_upload`-Dropdown-Optionen (Reiter „Upload Savegame") — fehlt darin das
    /// aktuell geladene Savegame, live verifiziert (kann nicht überschrieben werden, während es
    /// läuft). Bewusst **nicht** aus `savegames` synthetisiert, s. `list_savegame_upload_slots`.
    upload_slots: Vec<FieldOption>,
}

#[tauri::command]
async fn savegames_view(state: tauri::State<'_, AppState>) -> Result<SavegamesView, String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    let online = matches!(
        sc.state().await.map_err(|e| e.to_string())?,
        ServerState::Online { .. }
    );
    let savegames = sc.list_savegames().await.map_err(|e| e.to_string())?;
    let upload_slots = sc
        .list_savegame_upload_slots()
        .await
        .map_err(|e| e.to_string())?;
    Ok(SavegamesView {
        online,
        savegames,
        upload_slots,
    })
}

/// Zeitstempel-Backups eines Slots (Reiter „Restore Savegame Backup").
#[tauri::command]
async fn list_savegame_backups(
    state: tauri::State<'_, AppState>,
    slot: u8,
) -> Result<Vec<SavegameBackup>, String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    sc.list_savegame_backups(slot).await.map_err(|e| e.to_string())
}

/// Savegame herunterladen — `path` kommt vom nativen Speichern-Dialog (Frontend), rein lesend
/// (kein Bestätigungsdialog nötig).
#[tauri::command]
async fn download_savegame(
    state: tauri::State<'_, AppState>,
    slot: u8,
    path: String,
) -> Result<(), String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    sc.download_savegame(slot, std::path::Path::new(&path))
        .await
        .map_err(|e| e.to_string())
}

/// `progress`-Event-Payload für einen laufenden Savegame-Upload — eigener Event-Name, damit er
/// sich nicht mit dem Mod-Upload-Fortschritt (`progress`) vermischt.
#[derive(Clone, Serialize)]
struct SavegameUploadProgress {
    done: u64,
    total: Option<u64>,
}

/// Savegame über das Web-Formular hochladen (G7, 7.9a) — bis 1,71 GB, sonst `Error::NoFileAccess`
/// (FTP/SFTP-Weg noch nicht umgesetzt). `slot` ist ein belegter **oder** leerer Ziel-Slot;
/// `name` der optionale `custom_name`. Fortschritt gedrosselt wie beim Mod-Upload.
#[tauri::command]
async fn upload_savegame(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    slot: u8,
    name: Option<String>,
    path: String,
) -> Result<(), String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;

    let last_emitted = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let app_handle = app.clone();
    sc.upload_savegame_with_progress(
        slot,
        name.as_deref(),
        std::path::Path::new(&path),
        move |p| {
            let total = p.total.unwrap_or(0);
            let step = (total / 100).max(262_144);
            let prev = last_emitted.load(std::sync::atomic::Ordering::Relaxed);
            if p.done.saturating_sub(prev) >= step || Some(p.done) == p.total {
                last_emitted.store(p.done, std::sync::atomic::Ordering::Relaxed);
                let _ = app_handle.emit(
                    "savegame-progress",
                    SavegameUploadProgress {
                        done: p.done,
                        total: p.total,
                    },
                );
            }
        },
    )
    .await
    .map_err(|e| e.to_string())
}

/// Savegame löschen (G7, destruktiv).
#[tauri::command]
async fn delete_savegame(state: tauri::State<'_, AppState>, slot: u8) -> Result<(), String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    sc.delete_savegame(slot).await.map_err(|e| e.to_string())
}

/// Zeitstempel-Backup wiederherstellen (G7) — überschreibt den Slot vollständig.
#[tauri::command]
async fn restore_savegame_backup(
    state: tauri::State<'_, AppState>,
    backup: SavegameBackup,
) -> Result<(), String> {
    let guard = state.sc.lock().await;
    let sc = &guard.as_ref().ok_or("Nicht verbunden")?.1;
    sc.restore_savegame_backup(&backup)
        .await
        .map_err(|e| e.to_string())
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
        .plugin(tauri_plugin_dialog::init())
        // "Auf ModHub ansehen"-Link bei Suche/Kategorie-Browsen: öffnet im System-Browser statt
        // (unbrauchbar) im App-Fenster selbst.
        .plugin(tauri_plugin_opener::init())
        // Fenstergröße/-position merken und beim nächsten Start wiederherstellen.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            // Auch im Release-Build ins Log-Verzeichnis schreiben (nicht nur im Debug-Build wie
            // bisher) — sonst gibt es bei einem gemeldeten Problem (z. B. „Server nicht
            // erreichbar") nichts, was man sich nachträglich ansehen könnte. Im Debug-Build
            // zusätzlich auf stdout, für die Entwicklung.
            use tauri_plugin_log::{Target, TargetKind};
            let mut builder = tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .target(Target::new(TargetKind::LogDir { file_name: None }));
            if cfg!(debug_assertions) {
                builder = builder.target(Target::new(TargetKind::Stdout));
            }
            app.handle().plugin(builder.build())?;
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
            save_settings,
            upload_mod,
            overwrite_mod,
            plan_uploads,
            list_mod_files,
            modhub_search,
            modhub_install,
            modhub_cancel,
            modhub_browse_category,
            savegames_view,
            list_savegame_backups,
            download_savegame,
            upload_savegame,
            delete_savegame,
            restore_savegame_backup
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
