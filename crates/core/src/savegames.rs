//! Savegame-Verwaltung: Liste, Backups (`savegames.html`, Kap. 7.8 LH). Upload/Download/Löschen/
//! Restore laufen wie bei Mods über das native Web-Formular, nicht über FTP/SFTP (Kap. 1.3 LH).
//!
//! **Tabelle live verifiziert** (Browser-DevTools, 2026-08-14): jeder belegte Slot ist ein
//! `div.container-row.grid-row` mit fünf Label/Wert-Spalten — Slot/Map/Money/Play Time/
//! Difficulty, gleiches Muster wie bei `mods.html`: `div.col-lg-hidden` trägt `<b>Label</b>`,
//! das Geschwister mit Klasse `col-lg-12` im selben Elternelement den Wert — plus einem
//! Download-Link `<a href="savegame<slot>">` und einem Lösch-Link
//! `<a href="savegames.html?delete_<slot>=true&…">`.
//!
//! **Der Download-Link liefert die zuverlässigste Slot-Kennung** (Zahl direkt aus der URL) —
//! robuster als die Slot-Spalte, deren Anzeigetext bei einem `custom_name`-Upload vom
//! Original "My game save (N)" abweicht.

use std::collections::HashMap;

use scraper::{ElementRef, Html, Selector};

use crate::model::{Difficulty, FieldOption, SavegameBackup, ServerSavegame};

/// Belegte Savegame-Slots von `savegames.html` lesen (Kap. 7.8 LH). Leere Slots erscheinen hier
/// nicht — die kommen nur als Ziel-Optionen im Upload-Formular vor (Kap. 6.1 PH).
pub(crate) fn parse_savegames(html: &str) -> Vec<ServerSavegame> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("div.container-row.grid-row").unwrap();
    let link_sel = Selector::parse("a[href]").unwrap();
    let delete_sel = Selector::parse(r#"a[href*="delete_"]"#).unwrap();

    let mut out = Vec::new();
    for row in doc.select(&row_sel) {
        // Nur Zeilen mit Download-Link sind echte Savegame-Zeilen — filtert Kopfzeile heraus.
        let Some(slot) = row
            .select(&link_sel)
            .find_map(|a| a.value().attr("href").and_then(slot_from_download_href))
        else {
            continue;
        };
        let cols = row_columns(row);
        out.push(ServerSavegame {
            slot,
            display_name: cols.get("Slot").cloned().unwrap_or_default(),
            map: cols.get("Map").cloned().unwrap_or_default(),
            money: cols.get("Money").and_then(|s| parse_money(s)).unwrap_or(0),
            play_time_minutes: cols
                .get("Play Time (hh:mm)")
                .and_then(|s| parse_play_time(s))
                .unwrap_or(0),
            difficulty: cols
                .get("Difficulty")
                .and_then(|s| Difficulty::from_label(s))
                .unwrap_or(Difficulty::Normal),
            // Live verifiziert (2026-08-14, laufender Server): das gerade geladene Savegame
            // trägt in seiner Zeile **keinen** Lösch-Link, alle anderen Slots weiterhin schon.
            can_delete: row.select(&delete_sel).next().is_some(),
        });
    }
    out
}

/// Ziel-Slots des Upload-Formulars lesen — die **echten** `<option>`-Werte des
/// `index_upload`-Dropdowns auf `savegames.html`, nicht synthetisch 1..20 nachgebaut.
///
/// **Live verifiziert (2026-08-14, laufender Server):** Das gerade geladene Savegame fehlt in
/// diesem Dropdown vollständig — man kann es nicht überschreiben, während es läuft. Das lässt
/// sich nicht aus [`parse_savegames`] herleiten (welcher Slot „gerade läuft" steht dort nicht
/// drin) und muss deshalb aus genau dieser Quelle kommen, nicht nachgebaut werden.
pub(crate) fn parse_upload_slot_options(html: &str) -> Vec<FieldOption> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(r#"select[name="index_upload"] option"#).unwrap();
    doc.select(&sel)
        .filter_map(|opt| {
            let value = opt.value().attr("value")?.to_string();
            let label = opt.text().collect::<String>().trim().to_string();
            Some(FieldOption { value, label })
        })
        .collect()
}

/// `savegame<slot>` → Slotnummer. `None` für alles andere (z. B. der Lösch-Link
/// `savegames.html?delete_1=true&…`, der zwar auch mit „savegame" beginnt, aber danach kein
/// reiner Ziffernrest ist).
fn slot_from_download_href(href: &str) -> Option<u8> {
    let rest = href.strip_prefix("savegame")?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// Label/Wert-Paare einer Savegame-Zeile: jedes `div.col-lg-hidden` trägt ein `<b>Label</b>`,
/// das Geschwister mit Klasse `col-lg-12` (im selben Elternelement) den Wert. Identisches Muster
/// zu `mods.rs::row_columns`, hier für die Slot-Tabelle. Spalten ohne Beschriftung (die
/// Aktions-Spalte mit Lösch-/Download-Link) liefern keinen Eintrag.
fn row_columns(row: ElementRef) -> HashMap<String, String> {
    let label_sel = Selector::parse("div.col-lg-hidden").unwrap();
    let mut cols = HashMap::new();
    for label_div in row.select(&label_sel) {
        let label = label_div.text().collect::<String>().trim().to_string();
        if label.is_empty() {
            continue; // Aktions-Spalte: kein <b>-Label, nur ein Icon-Link.
        }
        let Some(parent) = label_div.parent().and_then(ElementRef::wrap) else {
            continue;
        };
        let value = parent.children().filter_map(ElementRef::wrap).find(|c| {
            c.value()
                .attr("class")
                .is_some_and(|cl| cl.split_whitespace().any(|t| t == "col-lg-12"))
        });
        if let Some(value) = value {
            cols.insert(label, value.text().collect::<String>().trim().to_string());
        }
    }
    cols
}

/// „500'000 $" → 500000. Tausendertrennzeichen (`'`) und Währungszeichen werden verworfen,
/// übrig bleiben nur die Ziffern.
fn parse_money(s: &str) -> Option<u64> {
    let digits: String = s.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// „39:26" (hh:mm) → Minuten.
fn parse_play_time(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let h: u32 = h.trim().parse().ok()?;
    let m: u32 = m.trim().parse().ok()?;
    Some(h * 60 + m)
}

/// Zeitstempel-Backups aller Slots von `savegames.html` lesen (Kap. 7.8 LH) — ein `<option>` im
/// `backup_restore`-Dropdown je Backup, `value="<slot>_<timestamp>"`. Der Aufrufer filtert bei
/// Bedarf nach `slot` (die Bibliothek liest hier bewusst alle auf einmal, da nur ein Formular
/// existiert statt einer Seite je Slot).
pub(crate) fn parse_backups(html: &str) -> Vec<SavegameBackup> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(r#"select[name="backup_restore"] option"#).unwrap();

    let mut out = Vec::new();
    for opt in doc.select(&sel) {
        let Some(value) = opt.value().attr("value") else {
            continue;
        };
        let Some((slot_str, timestamp)) = value.split_once('_') else {
            continue;
        };
        let Ok(slot) = slot_str.parse::<u8>() else {
            continue;
        };
        let text = opt.text().collect::<String>();
        let (map, play_time_minutes) = parse_backup_summary(&text);
        out.push(SavegameBackup {
            slot,
            timestamp: timestamp.to_string(),
            map: map.unwrap_or_default(),
            play_time_minutes,
        });
    }
    out
}

/// Aus „Savegame 2 (2026-07-13_23-56) - Map: NF Marsch 4fach, Play Time: 39:26 hh:mm" Map und
/// Spielzeit lesen. Fehlt ein Teil (Layoutänderung), bleibt er leer/`0` statt abzubrechen — die
/// Kennung (`slot`/`timestamp`, aus dem `value`-Attribut) ist davon unabhängig und bleibt sicher.
fn parse_backup_summary(text: &str) -> (Option<String>, u32) {
    let Some(after_map) = text.split_once("Map: ").map(|(_, rest)| rest) else {
        return (None, 0);
    };
    let Some((map, rest)) = after_map.split_once(", Play Time: ") else {
        return (Some(after_map.trim().to_string()), 0);
    };
    let minutes = rest
        .split_whitespace()
        .next()
        .and_then(parse_play_time)
        .unwrap_or(0);
    (Some(map.trim().to_string()), minutes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Difficulty;

    // Echtes Markup von `savegames.html`, live erfasst 2026-08-14 (Browser-DevTools) an einem
    // **laufenden** Server, auf zwei Slots reduziert. Slot 1: normaler, nicht geladener Slot —
    // hat trotz laufendem Server einen Lösch-Link. Slot 4: das **aktuell geladene** Savegame —
    // seine Zeile hat live verifiziert **keinen** Lösch-Link (kann sich nicht selbst löschen).
    const HTML: &str = r#"
      <div class="container table-grid table2">
       <div class="container-row col-lg-visible col-xs-hidden grid-row">
           <div class="col col-lg-2"><b>Slot</b></div>
           <div class="col col-lg-3"><b>Map</b></div>
           <div class="col col-lg-2"><b>Money</b></div>
           <div class="col col-lg-1"><b>Play Time (hh:mm)</b></div>
           <div class="col col-lg-2"><b>Difficulty</b></div>
       </div>
       <div class="container-row grid-row">
           <div class="container-row col col-lg-2 col-xs-12 word-break-all" title="My game save (1)">
               <div class="col col-lg-hidden col-xs-6"><b>Slot</b></div>
               <div class="col col-lg-12 col-xs-6">My game save (1)</div>
           </div>
           <div class="container-row col col-lg-3 col-xs-12 word-break-all" title="Riverbend Springs">
               <div class="col col-lg-hidden col-xs-6"><b>Map</b></div>
               <div class="col col-lg-12 col-xs-6">Riverbend Springs</div>
           </div>
           <div class="container-row col col-lg-2 col-xs-12">
               <div class="col col-lg-hidden col-xs-6"><b>Money</b></div>
               <div class="col col-lg-12 col-xs-6">500'000 $</div>
           </div>
           <div class="container-row col col-lg-1 col-xs-12">
               <div class="col col-lg-hidden col-xs-6"><b>Play Time (hh:mm)</b></div>
               <div class="col col-lg-12 col-xs-6">00:24</div>
           </div>
           <div class="container-row col col-lg-2 col-xs-12" title="Riverbend Springs">
               <div class="col col-lg-hidden col-xs-6"><b>Difficulty</b></div>
               <div class="col col-lg-12 col-xs-6">Normal</div>
           </div>
           <div class="container-row col col-lg-1 col-xs-12">
               <div class="col col-lg-hidden col-xs-6"><a title="Delete My game save (1) " href="savegames.html?delete_1=true&amp;lang=en"><img class="icon" src="img/icons/deleteIcon.png"></a></div>
               <div class="col col-lg-12 col-xs-6"><a title="Download My game save (1)" href="savegame1"><img class="icon" src="img/icons/saveIcon.png"></a></div>
           </div>
           <div class="container-row col col-lg-1 col-lg-visible col-xs-hidden">
               <div class="col col-lg-12 col-xs-6"><a title="Delete My game save (1) " href="savegames.html?delete_1=true&amp;lang=en"><img class="icon" src="img/icons/deleteIcon.png"></a></div>
           </div>
       </div>
       <div class="container-row grid-row">
           <div class="container-row col col-lg-2 col-xs-12 word-break-all" title="My game save (4)">
               <div class="col col-lg-hidden col-xs-6"><b>Slot</b></div>
               <div class="col col-lg-12 col-xs-6">My game save (4)</div>
           </div>
           <div class="container-row col col-lg-3 col-xs-12 word-break-all" title="DEUTSCHLAND">
               <div class="col col-lg-hidden col-xs-6"><b>Map</b></div>
               <div class="col col-lg-12 col-xs-6">DEUTSCHLAND</div>
           </div>
           <div class="container-row col col-lg-2 col-xs-12">
               <div class="col col-lg-hidden col-xs-6"><b>Money</b></div>
               <div class="col col-lg-12 col-xs-6">93'839'499 $</div>
           </div>
           <div class="container-row col col-lg-1 col-xs-12">
               <div class="col col-lg-hidden col-xs-6"><b>Play Time (hh:mm)</b></div>
               <div class="col col-lg-12 col-xs-6">01:55</div>
           </div>
           <div class="container-row col col-lg-2 col-xs-12" title="DEUTSCHLAND">
               <div class="col col-lg-hidden col-xs-6"><b>Difficulty</b></div>
               <div class="col col-lg-12 col-xs-6">Easy</div>
           </div>
           <div class="container-row col col-lg-1 col-xs-12">
               <div class="col col-lg-12 col-xs-6"><a title="Download My game save (4)" href="savegame4"><img class="icon" src="img/icons/saveIcon.png"></a></div>
           </div>
       </div>
      </div>"#;

    #[test]
    fn liest_belegte_slots() {
        let saves = parse_savegames(HTML);
        assert_eq!(saves.len(), 2);
        let s1 = &saves[0];
        assert_eq!(s1.slot, 1);
        assert_eq!(s1.display_name, "My game save (1)");
        assert_eq!(s1.map, "Riverbend Springs");
        assert_eq!(s1.money, 500_000);
        assert_eq!(s1.play_time_minutes, 24);
        assert_eq!(s1.difficulty, Difficulty::Normal);
        assert!(
            s1.can_delete,
            "normaler Slot hat trotz laufendem Server einen Lösch-Link"
        );
    }

    #[test]
    fn aktuell_geladenes_savegame_hat_keinen_loeschlink() {
        // Slot 4 hat keinen Lösch-Link im Test-Markup (das aktuell geladene Savegame kann sich
        // nicht selbst löschen, live verifiziert) — die Slot-Erkennung hängt am Download-Link,
        // nicht am Lösch-Link, also unbeeinflusst davon.
        let saves = parse_savegames(HTML);
        let s4 = &saves[1];
        assert_eq!(s4.slot, 4);
        assert_eq!(s4.map, "DEUTSCHLAND");
        assert_eq!(s4.money, 93_839_499);
        assert_eq!(s4.play_time_minutes, 115);
        assert_eq!(s4.difficulty, Difficulty::Easy);
        assert!(!s4.can_delete);
    }

    #[test]
    fn ohne_tabelle_leere_liste() {
        assert!(parse_savegames("<html><body>nichts</body></html>").is_empty());
    }

    #[test]
    fn slot_aus_download_href() {
        assert_eq!(slot_from_download_href("savegame1"), Some(1));
        assert_eq!(slot_from_download_href("savegame20"), Some(20));
        assert_eq!(
            slot_from_download_href("savegames.html?delete_1=true&lang=en"),
            None
        );
        assert_eq!(slot_from_download_href("index.html"), None);
    }

    #[test]
    fn geld_und_spielzeit_parsen() {
        assert_eq!(parse_money("500'000 $"), Some(500_000));
        assert_eq!(parse_money("93'839'499 $"), Some(93_839_499));
        assert_eq!(parse_money(""), None);
        assert_eq!(parse_play_time("00:24"), Some(24));
        assert_eq!(parse_play_time("39:26"), Some(39 * 60 + 26));
        assert_eq!(parse_play_time("bad"), None);
    }

    // Echtes `index_upload`-Dropdown, live erfasst 2026-08-14 an einem **laufenden** Server, auf
    // wenige Einträge reduziert. Server hat vier Slots belegt (1, 2, 3 leer, 4 = aktuell
    // geladen) — Slot 4 fehlt hier bewusst, das ist keine Auslassung im Test-Fixture.
    const UPLOAD_OPTIONS_HTML: &str = r#"
      <select name="index_upload">
        <option value="1">My game save (1)</option>
        <option value="2">My game save (2)</option>
        <option value="3">SAVEGAME 3</option>
        <option value="5">SAVEGAME 5</option>
      </select>"#;

    #[test]
    fn liest_upload_dropdown_ohne_das_laufende_savegame() {
        // Live verifiziert: Slot 4 läuft gerade und fehlt deshalb im Dropdown — das lässt sich
        // nicht aus `parse_savegames` herleiten, nur aus dieser echten Formularquelle.
        let options = parse_upload_slot_options(UPLOAD_OPTIONS_HTML);
        assert_eq!(options.len(), 4);
        assert_eq!(options[0].value, "1");
        assert_eq!(options[0].label, "My game save (1)");
        assert_eq!(options[2].value, "3");
        assert_eq!(options[2].label, "SAVEGAME 3");
        assert!(!options.iter().any(|o| o.value == "4"));
    }

    #[test]
    fn ohne_upload_dropdown_leere_liste() {
        assert!(parse_upload_slot_options("<html><body>nichts</body></html>").is_empty());
    }

    // Echtes `backup_restore`-Dropdown, live erfasst 2026-08-14, auf zwei Einträge reduziert.
    const BACKUP_HTML: &str = r#"
      <select name="backup_restore">
        <option value="2_2026-07-13_23-56">Savegame 2 (2026-07-13_23-56) - Map: NF Marsch 4fach, Play Time: 39:26 hh:mm</option>
        <option value="4_2026-08-11_04-58">Savegame 4 (2026-08-11_04-58) - Map: DEUTSCHLAND, Play Time: 01:55 hh:mm</option>
      </select>"#;

    #[test]
    fn liest_backups() {
        let backups = parse_backups(BACKUP_HTML);
        assert_eq!(backups.len(), 2);
        assert_eq!(backups[0].slot, 2);
        assert_eq!(backups[0].timestamp, "2026-07-13_23-56");
        assert_eq!(backups[0].map, "NF Marsch 4fach");
        assert_eq!(backups[0].play_time_minutes, 39 * 60 + 26);
        assert_eq!(backups[1].slot, 4);
        assert_eq!(backups[1].timestamp, "2026-08-11_04-58");
    }

    #[test]
    fn backup_wert_ist_das_formularfeld() {
        // `backup_restore=<slot>_<timestamp>` muss sich unverändert aus slot+timestamp
        // zurückbauen lassen — das ist der Wert, den `restore_savegame_backup` sendet.
        let backups = parse_backups(BACKUP_HTML);
        let b = &backups[0];
        assert_eq!(format!("{}_{}", b.slot, b.timestamp), "2_2026-07-13_23-56");
    }

    #[test]
    fn ohne_dropdown_leere_liste() {
        assert!(parse_backups("<html><body>nichts</body></html>").is_empty());
    }
}
