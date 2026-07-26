//! Mod-Liste lesen, aktivieren/deaktivieren, Upload (Web/FTP), Löschen (Kap. 7.3 LH).
//! Umschalten/Löschen nur bei gestopptem Server (folgt).
//!
//! **Mod-Liste parsen (am lebenden Server verifiziert):** Jede Mod-Zeile ist ein `div.grid-row`
//! mit einer Checkbox `input.modSelection`. Deren `name` liefert **Dateiname und Status** in einem:
//! `moddeactivate_<Datei>` = derzeit **aktiv** (die Checkbox würde deaktivieren),
//! `modactivate_<Datei>` = **inaktiv**. Das ist robuster als das `id`-Attribut der Zeile (das haben
//! nur die aktiven) oder die `modSelection-…`-Klasse (nur Auswahl-Zustand). Felder stehen als
//! Label→Wert-Paare (`col-xs-3 col-md-hidden` = Label, `col-xs-9 col-md-12` = Wert); der sichtbare
//! „Filename" ist teils gekürzt und wird ignoriert.
//!
//! Offen: `ModStatus::Orphan` (Karteileiche = Registry-Eintrag ohne Datei) wird noch **nicht**
//! erkannt — dafür fehlt bislang ein echtes Beispiel am Server (dieser hat keine).

use scraper::{ElementRef, Html, Selector};

use crate::model::{ModStatus, ServerMod};

/// Aktive und inaktive Mods aus der Home-Seite lesen (Pflichtenheft 10.5 LH).
pub(crate) fn parse_mods(html: &str) -> Vec<ServerMod> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("div.grid-row").unwrap();
    doc.select(&row_sel).filter_map(parse_row).collect()
}

/// Eine Mod-Zeile auswerten. Dateiname und Status kommen aus dem Checkbox-`name`
/// (`mod(de)activate_<Datei>`); die übrigen Felder aus den Label→Wert-Paaren. Zeilen ohne
/// Mod-Checkbox (z. B. die Kopfzeile) liefern `None`.
fn parse_row(row: ElementRef) -> Option<ServerMod> {
    let cb_sel = Selector::parse("input.modSelection[name]").unwrap();
    let cb_name = row.select(&cb_sel).next()?.value().attr("name")?;
    let (status, file_name) = if let Some(f) = cb_name.strip_prefix("moddeactivate_") {
        (ModStatus::Active, f.to_string())
    } else {
        let f = cb_name.strip_prefix("modactivate_")?;
        (ModStatus::Inactive, f.to_string())
    };

    let pair_sel = Selector::parse("div.col-xs-3.col-md-hidden, div.col-xs-9.col-md-12").unwrap();
    let (mut display_name, mut version, mut author, mut size) = (None, None, None, None);
    let mut label: Option<String> = None;
    for el in row.select(&pair_sel) {
        let class = el.value().attr("class").unwrap_or("");
        let text = el.text().collect::<String>().trim().to_string();
        if class.contains("col-xs-3") {
            // Label-Spalte
            label = Some(text.to_lowercase());
        } else {
            // Wert-Spalte — dem zuletzt gesehenen Label zuordnen
            match label.take().as_deref() {
                Some("name") => display_name = non_empty(text),
                Some("version") => version = non_empty(text),
                Some("author") => author = non_empty(text),
                Some("size") => size = parse_size(&text),
                _ => {} // „filename" ignorieren (gekürzt; Dateiname kommt aus der Checkbox)
            }
        }
    }

    Some(ServerMod {
        is_dlc: file_name.to_ascii_lowercase().ends_with(".dlc"),
        file_name,
        display_name,
        version,
        author,
        size,
        status,
    })
}

fn non_empty(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

/// „393.37 MB" → Bytes. `None`, wenn nicht als `<Zahl> <Einheit>` erkennbar.
fn parse_size(s: &str) -> Option<u64> {
    let (num, unit) = s.trim().split_once(' ')?;
    let value: f64 = num.trim().parse().ok()?;
    let mult = match unit.trim().to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "KB" => 1024.0,
        "MB" => 1024.0 * 1024.0,
        "GB" => 1024.0 * 1024.0 * 1024.0,
        "TB" => 1024.0_f64.powi(4),
        _ => return None,
    };
    Some((value * mult) as u64)
}

#[cfg(test)]
mod tests {
    use super::parse_mods;
    use crate::model::ModStatus;

    // Echtes Markup des FS25-Panels, auf zwei Zeilen reduziert (öffentliche Mod-Namen).
    const HTML: &str = r#"
      <form name="ActiveMods" action="index.html?lang=en#mods" method="post">
        <div class="container-row col-md-visible col-xs-hidden grid-row">
          <div class="col col-md-3"><b>Name</b></div>
        </div>
        <div class="container-row grid-row modSelection-inactive" id="FS25_DashboardLive_VanillaVehicles.zip">
          <div class="col col-xs-5 col-md-12"><input type="checkbox" class="modSelection"
             name="moddeactivate_FS25_DashboardLive_VanillaVehicles.zip"></div>
          <div class="container-row col col-md-3 col-xs-12">
            <div class="col col-xs-3 col-md-hidden"> Name</div>
            <div class="col col-xs-9 col-md-12">Dashboard Live Vanilla Vehicles</div>
          </div>
          <div class="container-row col col-md-1 col-xs-12">
            <div class="col col-xs-3 col-md-hidden">Version</div>
            <div class="col col-xs-9 col-md-12">1.0.0.0             </div>
          </div>
          <div class="container-row col col-md-2 col-xs-12">
            <div class="col col-xs-3 col-md-hidden">Author</div>
            <div class="col col-xs-9 col-md-12">Mister_mojo_AT, SbSh</div>
          </div>
          <div class="container-row col col-md-3 col-xs-12">
            <div class="col col-xs-3 col-md-hidden">Filename</div>
            <div class="col col-xs-9 col-md-12">FS25_DashboardLive_VanillaVehicl...</div>
          </div>
          <div class="container-row col col-md-2 col-xs-12">
            <div class="col col-xs-3 col-md-hidden">Size</div>
            <div class="col col-xs-9 col-md-12"><span class="float-right-md">393.37 MB</span></div>
          </div>
        </div>
      </form>
      <form name="InactiveMods" action="index.html?lang=en#mods" method="post">
        <div class="container-row grid-row modSelection-inactive">
          <div class="col col-xs-5 col-md-12"><input type="checkbox" class="modSelection" name="modactivate_nexatPack.dlc"></div>
          <div class="container-row col col-md-3 col-xs-12">
            <div class="col col-xs-3 col-md-hidden"> Name</div>
            <div class="col col-xs-9 col-md-12">NEXAT Pack</div>
          </div>
        </div>
      </form>"#;

    #[test]
    fn liest_aktive_und_inaktive_mods() {
        let mods = parse_mods(HTML);
        assert_eq!(mods.len(), 2);

        let active = &mods[0];
        assert_eq!(active.file_name, "FS25_DashboardLive_VanillaVehicles.zip");
        assert_eq!(active.status, ModStatus::Active);
        assert_eq!(
            active.display_name.as_deref(),
            Some("Dashboard Live Vanilla Vehicles")
        );
        assert_eq!(active.version.as_deref(), Some("1.0.0.0")); // getrimmt
        assert_eq!(active.author.as_deref(), Some("Mister_mojo_AT, SbSh"));
        assert_eq!(active.size, Some((393.37 * 1024.0 * 1024.0) as u64));
        assert!(!active.is_dlc);
    }

    #[test]
    fn inaktive_mod_und_dlc_erkennung() {
        let mods = parse_mods(HTML);
        let inactive = &mods[1];
        assert_eq!(inactive.file_name, "nexatPack.dlc");
        assert_eq!(inactive.status, ModStatus::Inactive);
        assert_eq!(inactive.display_name.as_deref(), Some("NEXAT Pack"));
        assert!(inactive.is_dlc); // .dlc-Endung
    }

    #[test]
    fn filename_kommt_aus_checkbox_nicht_aus_gekuerztem_feld() {
        // Das sichtbare Filename-Feld ist gekürzt („…Vehicl..."); der Checkbox-`name` ist voll.
        let mods = parse_mods(HTML);
        assert_eq!(mods[0].file_name, "FS25_DashboardLive_VanillaVehicles.zip");
    }

    #[test]
    fn ohne_mod_formulare_leere_liste() {
        assert!(parse_mods("<html><body>nichts</body></html>").is_empty());
    }
}
