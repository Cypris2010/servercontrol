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
use crate::model::{Difficulty, FieldOption, GameSettings, PauseIfEmpty, SettingsOptions};
use crate::secret::Secret;
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

/// Spiel-Einstellungen aus dem `configuration`-Formular lesen (Kap. 6.1). Funktioniert **bei
/// gestopptem Server**, wo die Felder als editierbares Formular vorliegen; online zeigt das Panel
/// die Werte nur als Text (kein Formular) → `FormMismatch`. `admin_password`/`game_password`
/// landen im `Secret` und werden nie geloggt.
pub(crate) fn parse_settings(html: &str) -> Result<GameSettings> {
    let doc = Html::parse_document(html);
    let form = doc
        .select(&Selector::parse(r#"form[name="configuration"]"#).unwrap())
        .next()
        .ok_or_else(|| Error::FormMismatch("configuration-Formular fehlt".to_string()))?;

    // Kernfeld als Formularfeld vorhanden? Sonst ist der Server vermutlich online (nur Textanzeige).
    if field_value(form, "game_name").is_none() {
        return Err(Error::FormMismatch(
            "Einstellungen nur bei gestopptem Server lesbar".to_string(),
        ));
    }
    let val = |name: &str| field_value(form, name);

    Ok(GameSettings {
        game_name: val("game_name").unwrap_or_default(),
        admin_password: Secret::new(val("admin_password").unwrap_or_default()),
        game_password: Secret::new(val("game_password").unwrap_or_default()),
        savegame: parse_num(form, "savegame").unwrap_or(1),
        map_start: val("map_start").unwrap_or_default(),
        initial_money: parse_num(form, "initialMoney").unwrap_or(0),
        initial_loan: parse_num(form, "initialLoan").unwrap_or(0),
        economic_difficulty: val("economicDifficulty")
            .and_then(|v| Difficulty::from_code(&v))
            .unwrap_or(Difficulty::Easy),
        server_port: parse_num(form, "server_port").unwrap_or(0),
        max_player: parse_num(form, "max_player").unwrap_or(0),
        mp_language: val("mp_language").unwrap_or_default(),
        auto_save_interval: parse_num(form, "auto_save_interval").unwrap_or(0),
        stats_interval: parse_num(form, "stats_interval").unwrap_or(0),
        pause_game_if_empty: val("pause_game_if_empty")
            .and_then(|v| PauseIfEmpty::from_code(&v))
            .unwrap_or(PauseIfEmpty::No),
        crossplay_allowed: checkbox_checked(form, "crossplay_allowed"),
    })
}

/// Verfügbare Dropdown-Optionen des `configuration`-Formulars lesen (G6). Nur bei gestopptem
/// Server, wo das echte Formular vorliegt (online → `FormMismatch`). Die Map-Liste ist
/// serverabhängig (Basis-Maps + installierte Map-Mods).
pub(crate) fn parse_settings_options(html: &str) -> Result<SettingsOptions> {
    let doc = Html::parse_document(html);
    let form = doc
        .select(&Selector::parse(r#"form[name="configuration"]"#).unwrap())
        .next()
        .ok_or_else(|| Error::FormMismatch("configuration-Formular fehlt".to_string()))?;
    if form
        .select(&Selector::parse(r#"select[name="savegame"]"#).unwrap())
        .next()
        .is_none()
    {
        return Err(Error::FormMismatch(
            "Optionen nur bei gestopptem Server lesbar".to_string(),
        ));
    }
    Ok(SettingsOptions {
        savegames: options_of(form, "savegame"),
        maps: options_of(form, "map_start"),
        initial_money: options_of(form, "initialMoney"),
        initial_loan: options_of(form, "initialLoan"),
        economic_difficulty: options_of(form, "economicDifficulty"),
        max_player: options_of(form, "max_player"),
        mp_language: options_of(form, "mp_language"),
        pause_game_if_empty: options_of(form, "pause_game_if_empty"),
    })
}

/// Alle `<option>` eines `<select name=…>` als `(value, label)` lesen. Optionen ohne `value`
/// werden übersprungen.
fn options_of(form: ElementRef, select_name: &str) -> Vec<FieldOption> {
    let sel = Selector::parse(&format!(r#"select[name="{select_name}"] option"#)).unwrap();
    form.select(&sel)
        .filter_map(|o| {
            let value = o.value().attr("value")?.to_string();
            let label = o.text().collect::<String>().trim().to_string();
            Some(FieldOption { value, label })
        })
        .collect()
}

/// Zahlenwert eines Formularfelds (generisch über den Zieltyp).
fn parse_num<T: std::str::FromStr>(form: ElementRef, name: &str) -> Option<T> {
    field_value(form, name).and_then(|v| v.trim().parse().ok())
}

/// Wert eines Formularfelds: `value` eines `<input>` oder die gewählte Option eines `<select>`.
fn field_value(form: ElementRef, name: &str) -> Option<String> {
    let input = Selector::parse(&format!(r#"input[name="{name}"]"#)).unwrap();
    if let Some(el) = form.select(&input).next() {
        return el.value().attr("value").map(str::to_string);
    }
    let opt = Selector::parse(&format!(r#"select[name="{name}"] option[selected]"#)).unwrap();
    form.select(&opt)
        .next()
        .and_then(|o| o.value().attr("value").map(str::to_string))
}

/// Ist die Checkbox `name` angehakt?
fn checkbox_checked(form: ElementRef, name: &str) -> bool {
    let sel = Selector::parse(&format!(r#"input[name="{name}"]"#)).unwrap();
    form.select(&sel)
        .next()
        .map(|e| e.value().attr("checked").is_some())
        .unwrap_or(false)
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

    #[test]
    fn liest_einstellungen() {
        use super::parse_settings;
        use crate::model::{Difficulty, PauseIfEmpty};
        // EXISTING um die restlichen Felder ergänzt (echtes Markup).
        let html = EXISTING.replace(
            r#"<input type="checkbox" name="crossplay_allowed" checked="checked">"#,
            r#"<input type="text" name="server_port" value="10823">
               <select name="mp_language"><option value="de" selected="selected">de</option></select>
               <input type="text" name="auto_save_interval" value="30">
               <input type="text" name="stats_interval" value="31536000">
               <select name="pause_game_if_empty"><option value="2" selected="selected">Instantly</option></select>
               <input type="checkbox" name="crossplay_allowed" checked="checked">"#,
        );
        let s = parse_settings(&html).unwrap();
        assert_eq!(s.game_name, "ccc222");
        assert_eq!(s.savegame, 2);
        assert_eq!(s.economic_difficulty, Difficulty::Easy); // "1"
        assert_eq!(s.server_port, 10823);
        assert_eq!(s.max_player, 4);
        assert_eq!(s.mp_language, "de");
        assert_eq!(s.auto_save_interval, 30);
        assert_eq!(s.stats_interval, 31_536_000);
        assert_eq!(s.pause_game_if_empty, PauseIfEmpty::Instantly); // "2"
        assert!(s.crossplay_allowed);
        // Passwörter gelesen, aber Debug zeigt sie nicht.
        assert_eq!(s.admin_password.expose(), "geheim1");
        assert_eq!(format!("{:?}", s.admin_password), "Secret(***)");
    }

    #[test]
    fn ohne_formular_fehler() {
        use super::parse_settings;
        assert!(parse_settings("<html><body>online, kein Formular</body></html>").is_err());
    }

    #[test]
    fn liest_dropdown_optionen() {
        use super::parse_settings_options;
        // Map-Select mit Basis-Maps + einer Map-Mod (echtes Markup).
        let html = r#"
          <form name="configuration">
            <select name="savegame"><option value="1">SAVEGAME 1 - Empty</option></select>
            <select name="map_start">
              <option value="default_MapEU">Zielonka</option>
              <option value="FS25_NFMarsch4fach.zip_SampleModMap" selected="selected">NF Marsch 4fach</option>
            </select>
          </form>"#;
        let o = parse_settings_options(html).unwrap();
        assert_eq!(o.maps.len(), 2);
        assert_eq!(o.maps[0].value, "default_MapEU");
        assert_eq!(o.maps[0].label, "Zielonka");
        assert_eq!(o.maps[1].value, "FS25_NFMarsch4fach.zip_SampleModMap");
        assert_eq!(o.maps[1].label, "NF Marsch 4fach");
        assert_eq!(o.savegames.len(), 1);
    }
}
