//! Spiel-Einstellungen lesen/schreiben und der **Voll-Formular-Umlauf** (Kap. 6).
//!
//! `start` und `save_settings` lesen das komplette `configuration`-Formular und senden **alle**
//! Felder unverändert zurück (nur der Absende-Knopf wird ergänzt), damit keine Einstellung
//! verlorengeht. Nachgebildetes Browserverhalten (am lebenden Server verifiziert):
//! - **savegame-abhängige Felder** (`map_start`, `initialMoney`, `initialLoan`,
//!   `economicDifficulty`) werden nur bei **leerem** Savegame gesendet — bei bestehendem sperrt
//!   sie das JS (`checkSavegame`), der Browser lässt sie weg. „Leer" = Options-Text matcht
//!   „SAVEGAME N - Empty" / „SPIELSTAND N - Leer" / „SAUVEGARDE N - Vide".
//! - **Checkboxen** nur wenn angehakt (`crossplay_allowed=on`); **Selects** die gewählte Option;
//!   **Absende-Knöpfe** (`save_settings`/`start_server`/…) werden NICHT übernommen.
//!
//! `admin_password`/`game_password` laufen als Werte durch, werden aber **nie** protokolliert
//! (der serialisierte Body wird nirgends ausgegeben).

use scraper::{ElementRef, Html, Selector};

use crate::error::Error;
use crate::Result;

/// Felder, die nur bei **leerem** Savegame editierbar sind (checkSavegame). Bei bestehendem
/// Savegame sendet der Browser sie nicht → wir lassen sie ebenfalls weg.
const SAVEGAME_ONLY: [&str; 4] = [
    "map_start",
    "initialMoney",
    "initialLoan",
    "economicDifficulty",
];

/// Alle „successful controls" des `configuration`-Formulars serialisieren (wie ein Browser).
/// Der Aufrufer ergänzt den passenden Absende-Knopf (`start_server`/`save_settings`).
pub(crate) fn configuration_body(html: &str) -> Result<Vec<(String, String)>> {
    let doc = Html::parse_document(html);
    let form = doc
        .select(&Selector::parse(r#"form[name="configuration"]"#).unwrap())
        .next()
        .ok_or_else(|| Error::FormMismatch("configuration-Formular fehlt".to_string()))?;

    let keep_savegame_fields = savegame_is_empty(form);
    let ctrl = Selector::parse("input, select, textarea").unwrap();
    let mut body = Vec::new();
    for el in form.select(&ctrl) {
        let v = el.value();
        let Some(name) = v.attr("name") else { continue };
        if !keep_savegame_fields && SAVEGAME_ONLY.contains(&name) {
            continue;
        }
        match v.name() {
            "input" => match v.attr("type").unwrap_or("text") {
                "submit" | "button" | "image" | "reset" => {} // Absende-Knöpfe raus
                "checkbox" | "radio" => {
                    if v.attr("checked").is_some() {
                        let value = v.attr("value").unwrap_or("on").to_string();
                        body.push((name.to_string(), value));
                    }
                }
                _ => body.push((name.to_string(), v.attr("value").unwrap_or("").to_string())),
            },
            "select" => {
                if let Some(value) = selected_option(el) {
                    body.push((name.to_string(), value));
                }
            }
            _ => body.push((name.to_string(), el.text().collect::<String>())), // textarea
        }
    }
    Ok(body)
}

/// Existiert im Formular `form_name` ein Steuerelement mit dem Namen `field`? (Versionstoleranz
/// Q4: erwarteter Absende-Knopf vor dem POST prüfen.)
pub(crate) fn form_has_field(html: &str, form_name: &str, field: &str) -> bool {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(&format!(r#"form[name="{form_name}"] [name="{field}"]"#)).unwrap();
    doc.select(&sel).next().is_some()
}

fn selected_option(select: ElementRef) -> Option<String> {
    let opt = Selector::parse("option[selected]").unwrap();
    select
        .select(&opt)
        .next()
        .and_then(|o| o.value().attr("value").map(str::to_string))
}

/// Ist das gewählte Savegame leer? (isEmptySavegame nachgebildet.)
fn savegame_is_empty(form: ElementRef) -> bool {
    let sel = Selector::parse(r#"select[name="savegame"] option[selected]"#).unwrap();
    form.select(&sel)
        .next()
        .map(|o| is_empty_label(&o.text().collect::<String>()))
        .unwrap_or(false)
}

fn is_empty_label(text: &str) -> bool {
    let t = text.trim();
    t.contains("- Empty") || t.contains("- Leer") || t.contains("- Vide")
}

#[cfg(test)]
mod tests {
    use super::{configuration_body, form_has_field};

    // configuration-Formular mit bestehendem Savegame (Auszug, echtes Markup, Passwort anonym).
    const EXISTING: &str = r#"
      <form name="configuration" action="index.html?lang=en" method="POST">
        <input type="text" name="game_name" value="ccc222">
        <input type="text" name="admin_password" value="geheim1">
        <input type="text" name="game_password" value="geheim2">
        <select name="savegame" id="savegame">
          <option value="1">SAVEGAME 1 - Map: Riverbend Springs</option>
          <option value="2" selected="selected">SAVEGAME 2 - Map: NF Marsch 4fach, Money: 1424614 $</option>
          <option value="3">SAVEGAME 3 - Empty</option>
        </select>
        <select name="map_start"><option value="A" selected="selected">A</option></select>
        <select name="initialMoney"><option value="X">X</option></select>
        <select name="economicDifficulty"><option value="1" selected="selected">1</option></select>
        <select name="max_player"><option value="4" selected="selected">4</option></select>
        <input type="checkbox" name="crossplay_allowed" checked="checked">
        <input type="submit" name="save_settings" value="Save">
        <input type="submit" name="start_server" value="Start">
      </form>"#;

    #[test]
    fn bestehendes_savegame_laesst_gesperrte_felder_weg() {
        let body = configuration_body(EXISTING).unwrap();
        let names: Vec<&str> = body.iter().map(|(k, _)| k.as_str()).collect();
        // enthalten: normale Felder
        assert!(names.contains(&"game_name"));
        assert!(names.contains(&"max_player"));
        // NICHT enthalten: savegame-abhängige Felder (Browser sendet sie bei belegtem Slot nicht)
        assert!(!names.contains(&"map_start"));
        assert!(!names.contains(&"initialMoney"));
        assert!(!names.contains(&"economicDifficulty"));
        // Absende-Knöpfe raus
        assert!(!names.contains(&"save_settings"));
        assert!(!names.contains(&"start_server"));
    }

    #[test]
    fn checkbox_als_on_selektierte_option_als_wert() {
        let body = configuration_body(EXISTING).unwrap();
        assert!(body.contains(&("crossplay_allowed".to_string(), "on".to_string())));
        assert!(body.contains(&("max_player".to_string(), "4".to_string())));
    }

    #[test]
    fn leeres_savegame_behaelt_die_felder() {
        let html = EXISTING.replace(
            r#"<option value="2" selected="selected">SAVEGAME 2 - Map: NF Marsch 4fach, Money: 1424614 $</option>"#,
            r#"<option value="2">SAVEGAME 2 - Map: NF Marsch 4fach</option>"#,
        )
        .replace(
            r#"<option value="3">SAVEGAME 3 - Empty</option>"#,
            r#"<option value="3" selected="selected">SAVEGAME 3 - Empty</option>"#,
        );
        let body = configuration_body(&html).unwrap();
        let names: Vec<&str> = body.iter().map(|(k, _)| k.as_str()).collect();
        assert!(names.contains(&"map_start")); // leeres Savegame → Felder editierbar → gesendet
        assert!(names.contains(&"economicDifficulty"));
    }

    #[test]
    fn form_has_field_erkennt_absendeknopf() {
        assert!(form_has_field(EXISTING, "configuration", "start_server"));
        assert!(!form_has_field(EXISTING, "configuration", "stop_server"));
    }
}
