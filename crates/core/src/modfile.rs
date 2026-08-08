//! Prüft lokale Dateien, **bevor** sie hochgeladen werden (G3, Kap. 7.6): steckt überhaupt ein
//! FS25-Mod drin (`modDesc.xml` im Zip — so erkennt das Spiel selbst einen Mod), und welche
//! Version deklariert er? Kein Netzwerkzugriff, nur lokales Lesen.

use std::fs::File;
use std::path::Path;

/// Ergebnis der lokalen Prüfung einer Datei.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalModInfo {
    /// Enthält das Zip eine `modDesc.xml`? Das ist der Anker, an dem das Spiel selbst einen Mod
    /// erkennt — ohne sie ist es kein FS25-Mod, unabhängig von der Dateiendung.
    pub is_fs25_mod: bool,
    /// Deklarierte Version, falls das `<version>`-Tag vorhanden und nicht leer ist.
    pub version: Option<String>,
}

/// Eine lokale Datei prüfen. Kein Zip / nicht lesbar → `is_fs25_mod: false`, `version: None`
/// (bewusst kein harter Fehler — die GUI zeigt das als Warnung, blockiert aber nicht generell).
pub fn inspect_local_mod(path: &Path) -> LocalModInfo {
    let Ok(file) = File::open(path) else {
        return LocalModInfo::default();
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return LocalModInfo::default();
    };
    let Ok(mut entry) = archive.by_name("modDesc.xml") else {
        return LocalModInfo::default();
    };
    let mut xml = String::new();
    if std::io::Read::read_to_string(&mut entry, &mut xml).is_err() {
        return LocalModInfo {
            is_fs25_mod: true,
            version: None,
        };
    }
    LocalModInfo {
        is_fs25_mod: true,
        version: extract_version(&xml),
    }
}

fn extract_version(xml: &str) -> Option<String> {
    let start = xml.find("<version>")? + "<version>".len();
    let end = start + xml[start..].find("</version>")?;
    let v = xml[start..end].trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::extract_version;

    #[test]
    fn liest_version_aus_moddesc() {
        let xml = r#"<?xml version="1.0"?><modDesc descVersion="96"><version>1.0.0.1</version></modDesc>"#;
        assert_eq!(extract_version(xml), Some("1.0.0.1".to_string()));
    }

    #[test]
    fn ohne_version_tag_none() {
        assert_eq!(extract_version("<modDesc></modDesc>"), None);
    }

    #[test]
    fn leeres_version_tag_none() {
        assert_eq!(
            extract_version("<modDesc><version></version></modDesc>"),
            None
        );
    }
}
