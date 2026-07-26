//! Live-Log per offset/epoch-Polling (`tail -f` über HTTP, MZ5 / Kap. 7.4 LH).
//!
//! **Am lebenden Server entschlüsselt:** `logs.html` trägt im JS die verfügbaren Logdateien
//! (`var logFiles = {'3': ['log_…txt', …], '0': […], '1': […]}`, je Typ neueste zuerst) und die
//! aktuelle `var logEpoch = N`. Der Typ steht im `<select name="log_type">` (0=Server,
//! 1=Webserver, 3=Game). Inkrementell gelesen wird per POST `log.json.longpoll` mit
//! `log_type&log_file&offset&epoch`; die Antwort ist JSON
//! `{"content": <base64>, "end_offset": N, "active": bool}`.

use base64::prelude::*;
use scraper::{Html, Selector};
use serde::Deserialize;
use std::collections::HashMap;

use crate::error::Error;
use crate::model::{LogChunk, LogListing, LogSource};
use crate::Result;

/// Endpunkt und Startwerte-Träger.
pub(crate) const LONGPOLL_ENDPOINT: &str = "log.json.longpoll";

/// Verfügbare Logs + aktuelle `epoch` aus `logs.html` lesen.
pub(crate) fn parse_listing(html: &str) -> Result<LogListing> {
    let epoch = extract_number(html, "var logEpoch = ").unwrap_or(1);
    let files_by_type = parse_log_files(html);
    let types = parse_log_types(html); // (code, Name)

    let sources = types
        .into_iter()
        .map(|(log_type, type_name)| LogSource {
            files: files_by_type.get(&log_type).cloned().unwrap_or_default(),
            log_type,
            type_name,
        })
        .collect();
    Ok(LogListing { epoch, sources })
}

/// JSON-Antwort des Longpoll-Endpunkts in einen `LogChunk` überführen (base64 → Text).
pub(crate) fn parse_chunk(json: &str) -> Result<LogChunk> {
    #[derive(Deserialize)]
    struct Resp {
        #[serde(default)]
        content: String,
        #[serde(default)]
        end_offset: u64,
        #[serde(default)]
        active: bool,
    }
    let resp: Resp =
        serde_json::from_str(json).map_err(|e| Error::Parse(format!("Log-JSON: {e}")))?;
    let bytes = BASE64_STANDARD
        .decode(resp.content.as_bytes())
        .map_err(|e| Error::Parse(format!("Log-base64: {e}")))?;
    Ok(LogChunk {
        content: String::from_utf8_lossy(&bytes).into_owned(),
        end_offset: resp.end_offset,
        active: resp.active,
    })
}

/// `var logEpoch = 7;` → 7. Liest die Zahl direkt hinter `marker`.
fn extract_number(html: &str, marker: &str) -> Option<u64> {
    let rest = &html[html.find(marker)? + marker.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// `var logFiles = {'3': ['a.txt','b.txt'], '0': [...]}` → Map Typ-Code → Dateien.
fn parse_log_files(html: &str) -> HashMap<u8, Vec<String>> {
    let Some(after) = html.split_once("var logFiles = ").map(|(_, r)| r) else {
        return HashMap::new();
    };
    // Das Objekt enthält nur `[...]`-Arrays (keine verschachtelten `{}`), daher ist das erste
    // `}` der Abschluss — robust gegen Leerzeichen/Umbrüche vor dem `;`.
    let Some(end) = after.find('}') else {
        return HashMap::new();
    };
    // JS-Objektliteral zu JSON machen: einfache→doppelte Anführungszeichen (Log-Dateinamen und
    // Typ-Schlüssel enthalten keine), und **trailing commas** entfernen (JS erlaubt `…",]`, JSON
    // nicht).
    let json = after[..=end]
        .replace('\'', "\"")
        .replace(",]", "]")
        .replace(",}", "}");
    let raw: HashMap<String, Vec<String>> = serde_json::from_str(&json).unwrap_or_default();
    raw.into_iter()
        .filter_map(|(k, v)| k.parse::<u8>().ok().map(|code| (code, v)))
        .collect()
}

/// `<select name="log_type">`-Optionen als (Code, Anzeigename).
fn parse_log_types(html: &str) -> Vec<(u8, String)> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(r#"select[name="log_type"] option"#).unwrap();
    doc.select(&sel)
        .filter_map(|o| {
            let code = o.value().attr("value")?.parse::<u8>().ok()?;
            Some((code, o.text().collect::<String>().trim().to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_chunk, parse_listing};

    const LOGS_HTML: &str = r#"
      <select name="log_type">
        <option value="3">Game</option>
        <option value="0">Server</option>
        <option value="1">Webserver</option>
      </select>
      <select id="log_file" name="log_file"></select>
      <script>
        var logFiles = {'3': ['log_2026-07-26.txt','log_2026-07-23.txt'], '0': ['server.log'], '1': []};
        var logType = 3;
        var logEpoch = 7;
        var logReadOffset = 1121;
      </script>"#;

    #[test]
    fn liest_logliste_und_epoch() {
        let l = parse_listing(LOGS_HTML).unwrap();
        assert_eq!(l.epoch, 7);
        assert_eq!(l.sources.len(), 3);
        let game = l.sources.iter().find(|s| s.log_type == 3).unwrap();
        assert_eq!(game.type_name, "Game");
        assert_eq!(game.files, vec!["log_2026-07-26.txt", "log_2026-07-23.txt"]);
        let web = l.sources.iter().find(|s| s.log_type == 1).unwrap();
        assert!(web.files.is_empty());
    }

    #[test]
    fn vertraegt_trailing_commas() {
        // Das FS-Panel schreibt JS-Arrays mit trailing comma (`…",]`) — muss trotzdem parsen.
        let html = r#"<select name="log_type"><option value="3">Game</option></select>
            <script>var logFiles = {'3': ['a.txt','b.txt',],'0': [],};var logEpoch = 2;</script>"#;
        let l = parse_listing(html).unwrap();
        assert_eq!(l.epoch, 2);
        let game = l.sources.iter().find(|s| s.log_type == 3).unwrap();
        assert_eq!(game.files, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn dekodiert_chunk_base64() {
        // "Hallo\n" base64 = "SGFsbG8K"
        let json = r#"{"_heartbeat":"","content":"SGFsbG8K","end_offset":6,"active":true}"#;
        let c = parse_chunk(json).unwrap();
        assert_eq!(c.content, "Hallo\n");
        assert_eq!(c.end_offset, 6);
        assert!(c.active);
    }

    #[test]
    fn leerer_chunk() {
        let json = r#"{"_heartbeat":"","content":"","end_offset":0,"active":false}"#;
        let c = parse_chunk(json).unwrap();
        assert_eq!(c.content, "");
        assert!(!c.active);
    }
}
