//! Persistenz für Profile und Anwendungseinstellungen (Pflichtenheft Kap. 8).
//!
//! **Nie** ein Passwort — nur `ServerProfile`/`AppSettings` als JSON, atomar geschrieben,
//! am plattformüblichen Konfigurationsort (`directories`-Crate, nicht geraten).

use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::profile::ServerProfile;
use crate::{Error, Result};

const CURRENT_SCHEMA_VERSION: u32 = 1;

fn config_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "ServerControl")
        .ok_or_else(|| Error::Storage("Konfigurationsordner nicht ermittelbar".to_string()))?;
    Ok(dirs.config_dir().to_path_buf())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfilesFile {
    schema_version: u32,
    profiles: Vec<ServerProfile>,
}

/// Anwendungseinstellungen (Kap. 8.1): zuletzt aktives Profil, Theme, Sprache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub active_profile: Option<Uuid>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            active_profile: None,
            theme: None,
            language: None,
        }
    }
}

/// Alle Profile laden. **Fehlende/kaputte Datei → leere Liste** (Kap. 8.5), kein harter Fehler.
pub fn load_profiles() -> Result<Vec<ServerProfile>> {
    let path = config_dir()?.join("profiles.json");
    match fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<ProfilesFile>(&raw) {
            Ok(f) if f.schema_version > CURRENT_SCHEMA_VERSION => Err(Error::Storage(format!(
                "profiles.json hat eine neuere Version ({}) als diese App unterstützt ({})",
                f.schema_version, CURRENT_SCHEMA_VERSION
            ))),
            Ok(f) => Ok(f.profiles),
            Err(_) => Ok(Vec::new()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(Error::Storage(e.to_string())),
    }
}

/// Alle Profile speichern (atomar, Kap. 8.5).
pub fn save_profiles(profiles: &[ServerProfile]) -> Result<()> {
    let file = ProfilesFile {
        schema_version: CURRENT_SCHEMA_VERSION,
        profiles: profiles.to_vec(),
    };
    write_json(&config_dir()?.join("profiles.json"), &file)
}

/// Anwendungseinstellungen laden. Fehlt die Datei, gelten die Vorgabewerte.
pub fn load_settings() -> Result<AppSettings> {
    let path = config_dir()?.join("settings.json");
    match fs::read_to_string(&path) {
        Ok(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppSettings::default()),
        Err(e) => Err(Error::Storage(e.to_string())),
    }
}

/// Anwendungseinstellungen speichern (atomar).
pub fn save_settings(settings: &AppSettings) -> Result<()> {
    write_json(&config_dir()?.join("settings.json"), settings)
}

/// Erst in eine temporäre Datei schreiben, dann umbenennen — kein halb geschriebener
/// Zustand bei Absturz/Stromausfall (Kap. 8.5). Rechte nur für den Nutzer lesbar.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| Error::Storage("ungültiger Ablagepfad".to_string()))?;
    fs::create_dir_all(dir).map_err(|e| Error::Storage(e.to_string()))?;
    let body = serde_json::to_string_pretty(value).map_err(|e| Error::Storage(e.to_string()))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &body).map_err(|e| Error::Storage(e.to_string()))?;
    set_owner_only(&tmp)?;
    fs::rename(&tmp, path).map_err(|e| Error::Storage(e.to_string()))?;
    Ok(())
}

#[cfg(unix)]
fn set_owner_only(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| Error::Storage(e.to_string()))
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> Result<()> {
    Ok(())
}
