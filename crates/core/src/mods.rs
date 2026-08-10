//! Mod-Liste lesen, aktivieren/deaktivieren, Upload (Web/FTP), Löschen (Kap. 7.3 LH).
//! Umschalten/Löschen nur bei gestopptem Server (folgt).
//!
//! **Mod-Liste parsen (in beiden Serverzuständen am lebenden Server verifiziert):** Jede Mod-Zeile
//! ist ein `div.grid-row` mit `modSelection`-Klasse. Dateiname und Status kommen — in dieser
//! Reihenfolge, je nach Verfügbarkeit:
//! - **Checkbox** `input.modSelection` (nur bei **gestopptem** Server): `moddeactivate_<Datei>` =
//!   aktiv, `modactivate_<Datei>` = inaktiv → Datei **und** Status in einem.
//! - **`id`-Attribut** der Zeile = voller Dateiname (haben nur die **aktiven** Zeilen, in beiden
//!   Zuständen). Bei **laufendem** Server fehlen die Checkboxen, daher wichtig.
//! - **Filename-Spalte** (Wertspalte 4) als letzter Ausweg (für inaktive Zeilen bei laufendem
//!   Server; kann bei sehr langen Namen gekürzt sein).
//!
//! Der **Status** kommt aus der Checkbox (offline eindeutig) oder — bei laufendem Server — aus dem
//! **Abschnitt**: die Überschrift „Active Mods" leitet den Aktiv-Bereich ein, „Activate Mods"
//! bzw. „Inactive Mods" den Inaktiv-Bereich. Die fünf Wertspalten (`col-xs-9 col-md-12`) stehen in
//! fester Reihenfolge: **Name, Version, Author, Filename, Size**.
//!
//! Offen: `ModStatus::Orphan` (Karteileiche = Registry-Eintrag ohne Datei) wird noch **nicht**
//! erkannt — dafür fehlt bislang ein echtes Beispiel am Server (dieser hat keine).

use scraper::{ElementRef, Html, Selector};

use crate::model::{ModStatus, ServerMod};

/// Aktive und inaktive Mods aus der Home-Seite lesen (Pflichtenheft 10.5 LH), in beiden
/// Serverzuständen. Überschriften und Mod-Zeilen werden **in Dokumentreihenfolge** durchlaufen,
/// damit der Abschnitt (Active/Inactive) den Status liefert, wenn keine Checkbox da ist.
pub(crate) fn parse_mods(html: &str) -> Vec<ServerMod> {
    let doc = Html::parse_document(html);
    let walk = Selector::parse("h2, div.grid-row").unwrap();
    let mut section = ModStatus::Active;
    let mut mods = Vec::new();
    for el in doc.select(&walk) {
        if el.value().name() == "h2" {
            let heading = el.text().collect::<String>();
            if heading.contains("Activate Mods") || heading.contains("Inactive Mods") {
                section = ModStatus::Inactive;
            } else if heading.contains("Active Mods") {
                section = ModStatus::Active;
            }
        } else if el
            .value()
            .attr("class")
            .is_some_and(|c| c.contains("modSelection"))
        {
            if let Some(m) = parse_row(el, section) {
                mods.push(m);
            }
        }
    }
    mods
}

/// Eine Mod-Zeile auswerten. `section` liefert den Status, falls keine Checkbox vorhanden ist.
fn parse_row(row: ElementRef, section: ModStatus) -> Option<ServerMod> {
    // Wertspalten in fester Reihenfolge: [Name, Version, Author, Filename, Size].
    let value_sel = Selector::parse("div.col-xs-9.col-md-12").unwrap();
    let value_cells: Vec<ElementRef> = row.select(&value_sel).collect();
    let vals: Vec<String> = value_cells
        .iter()
        .map(|e| e.text().collect::<String>().trim().to_string())
        .collect();
    let col = |i: usize| vals.get(i).and_then(|s| non_empty(s.clone()));

    // „Update verfügbar"-Icon (live verifiziert): steckt direkt in der Versionsspalte, als
    // `img[src*="updateIcon"]` — verlinkt nur auf die ModHub-Kategorie „Update", nicht auf eine
    // konkrete `mod_id` (die holt die GUI bei Bedarf separat über die Kategorieseite).
    let update_sel = Selector::parse(r#"img[src*="updateIcon"]"#).unwrap();
    let update_available = value_cells
        .get(1)
        .is_some_and(|v| v.select(&update_sel).next().is_some());

    // Dateiname + Status: Checkbox (offline) > id (aktive) / Filename-Spalte + Abschnitt.
    let cb = row
        .select(&Selector::parse("input.modSelection[name]").unwrap())
        .next()
        .and_then(|c| c.value().attr("name").map(str::to_string));
    let (status, file_name) = match cb.as_deref() {
        Some(n) if n.starts_with("moddeactivate_") => {
            (ModStatus::Active, n["moddeactivate_".len()..].to_string())
        }
        Some(n) if n.starts_with("modactivate_") => {
            (ModStatus::Inactive, n["modactivate_".len()..].to_string())
        }
        _ => {
            let file = row
                .value()
                .attr("id")
                .map(str::to_string)
                .or_else(|| col(3))?;
            (section, file)
        }
    };

    Some(ServerMod {
        is_dlc: file_name.to_ascii_lowercase().ends_with(".dlc"),
        file_name,
        display_name: col(0),
        version: col(1),
        author: col(2),
        size: vals.get(4).and_then(|s| parse_size(s)),
        status,
        update_available,
        // Home-Seite hat keine ModHub-/Issues-Spalten (nur `mods.html`, s. `parse_mods_page_indexed`).
        from_modhub: false,
        issue_count: 0,
        issues: Vec::new(),
    })
}

fn non_empty(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

/// Mod-Liste von `mods.html` lesen — direktere Quelle als die Home-Seite (Browser-DevTools
/// live geprüft, 2026-08-07): jede Mod-Zeile ist ein `div.grid-row` mit Label/Wert-Paaren
/// (`div.col-lg-hidden` trägt `<b>Label</b>`, das Geschwisterelement mit Klasse `col-lg-12`
/// den Wert), u. a. mit einer expliziten **`Active`-Spalte** (`Yes`/`No`) — anders als bei der
/// Home-Seite steckt der Status hier nicht in Checkbox-Namen oder der Abschnittsüberschrift.
///
/// Der **volle** Dateiname kommt vorrangig vom Löschknopf (`button[name="deleteactive"
/// |"deleteinactive"]`, `value` = voller Name) — die sichtbare Filename-Spalte kann wie bei
/// der Home-Seite gekürzt sein. Ohne Löschknopf (z. B. vermutlich bei laufendem Server, wo
/// Löschen nicht erlaubt ist — **nicht** live verifiziert) fällt es auf die `Active`-Spalte
/// und die (ggf. gekürzte) Filename-Spalte zurück.
///
/// Zeilen ohne `Filename`-Spalte (Tabellenkopf, ModHub-Katalogzeilen weiter unten auf der
/// Seite) werden übersprungen — live geprüft, dass Katalogzeilen nur `Name`/`Version`/`Author`
/// tragen, keine `Filename`-Spalte.
/// Eine von `mods.html` gelesene Mod, plus die `mod_index` aus dem Zeilen-Link
/// (`mod.html?mod_index=<i>`) — `None`, falls sich in der Zeile kein solcher Link findet.
/// Die Detailseite trägt Name/Autor/Dateiname **ungekürzt** (Kap. 10.5, Nachladen bei „…").
pub(crate) struct ParsedMod {
    pub(crate) info: ServerMod,
    pub(crate) mod_index: Option<u32>,
}

pub(crate) fn parse_mods_page_indexed(html: &str) -> Vec<ParsedMod> {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("div.grid-row").unwrap();
    let label_sel = Selector::parse("div.col-lg-hidden").unwrap();
    let update_sel = Selector::parse(r#"img[src*="updateIcon"]"#).unwrap();
    let delete_sel = Selector::parse(r#"button[type="submit"][name^="delete"]"#).unwrap();
    let index_sel = Selector::parse(r#"a[href*="mod_index="]"#).unwrap();

    let mut mods = Vec::new();
    for row in doc.select(&row_sel) {
        let cols = row_columns(row, &label_sel);
        let Some(short_file_name) = cols.get("Filename") else {
            continue; // Kopfzeile oder ModHub-Katalogzeile, keine installierte Mod.
        };

        let delete = row.select(&delete_sel).next().and_then(|b| {
            let name = b.value().attr("name")?;
            let value = b.value().attr("value")?;
            Some((name.to_string(), value.to_string()))
        });
        let (status, file_name) = match delete.as_ref().map(|(n, v)| (n.as_str(), v.as_str())) {
            Some(("deleteactive", value)) => (ModStatus::Active, value.to_string()),
            Some(("deleteinactive", value)) => (ModStatus::Inactive, value.to_string()),
            _ => {
                let status = match cols.get("Active").map(String::as_str) {
                    Some("Yes") => ModStatus::Active,
                    _ => ModStatus::Inactive,
                };
                (status, short_file_name.clone())
            }
        };

        let info = ServerMod {
            is_dlc: file_name.to_ascii_lowercase().ends_with(".dlc"),
            file_name,
            display_name: cols.get("Name").cloned(),
            version: cols.get("Version").cloned(),
            author: cols.get("Author").cloned(),
            size: cols.get("Size").and_then(|s| parse_size(s)),
            status,
            update_available: row.select(&update_sel).next().is_some(),
            from_modhub: cols.get("ModHub").map(String::as_str) == Some("Yes"),
            issue_count: cols
                .get("Issues")
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0),
            issues: Vec::new(), // nur bei Bedarf von der Detailseite nachgeladen, s. `needs_detail`.
        };
        let mod_index = row.select(&index_sel).next().and_then(mod_index_from_href);
        mods.push(ParsedMod { info, mod_index });
    }
    mods
}

/// `mod_index=<n>` aus einem `<a href="mod.html?lang=en&mod_index=3">`-Link lesen.
fn mod_index_from_href(a: ElementRef) -> Option<u32> {
    let href = a.value().attr("href")?;
    let after = href.split("mod_index=").nth(1)?;
    after.split('&').next()?.parse().ok()
}

/// Endet ein Feld auf `mods.html`/`index.html` gekürzt (`"…"` bzw. `"..."`)?
fn is_truncated(s: &str) -> bool {
    let s = s.trim_end();
    s.ends_with('…') || s.ends_with("...")
}

/// Muss für diese Mod die Detailseite nachgeladen werden — Name, Autor, Dateiname **oder**
/// Version gekürzt, **oder** es gibt Issues (deren Volltext nur dort steht, `mods.html` hat nur
/// die Anzahl)? Ein einziger Treffer reicht — die Detailseite liefert ohnehin alle Felder **und**
/// die Issues-Liste auf einmal, es wird also **pro Mod höchstens ein** zusätzlicher Request
/// gebraucht, nicht einer pro gekürztem Feld oder extra für die Issues.
pub(crate) fn needs_detail(m: &ServerMod) -> bool {
    m.display_name.as_deref().is_some_and(is_truncated)
        || m.author.as_deref().is_some_and(is_truncated)
        || m.version.as_deref().is_some_and(is_truncated)
        || is_truncated(&m.file_name)
        || m.issue_count > 0
}

/// Ungekürzte Felder und Issues-Liste von der Detailseite ([`parse_mod_detail`]) übernehmen —
/// bei Name/Autor/Dateiname/Version **nur**, wenn sie tatsächlich gekürzt waren; alles andere
/// (Status, Größe, `update_available`, `from_modhub`, …) bleibt von `mods.html` unverändert, da
/// nur dort verlässlich (Kap. 10.5). Issues werden übernommen, sobald welche gefunden wurden.
pub(crate) fn apply_detail(m: &mut ServerMod, detail: ModDetail) {
    if m.display_name.as_deref().is_some_and(is_truncated) {
        if let Some(v) = detail.display_name {
            m.display_name = Some(v);
        }
    }
    if m.author.as_deref().is_some_and(is_truncated) {
        if let Some(v) = detail.author {
            m.author = Some(v);
        }
    }
    if m.version.as_deref().is_some_and(is_truncated) {
        if let Some(v) = detail.version {
            m.version = Some(v);
        }
    }
    if is_truncated(&m.file_name) {
        if let Some(v) = detail.file_name {
            m.file_name = v;
        }
    }
    if !detail.issues.is_empty() {
        m.issues = detail.issues;
    }
}

/// Ungekürzte Felder und Issues-Volltext von `mod.html?mod_index=<i>` (Kap. 10.5, Nachladen bei
/// „…" bzw. bei `issue_count > 0`).
#[derive(Default)]
pub(crate) struct ModDetail {
    pub(crate) display_name: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) file_name: Option<String>,
    pub(crate) issues: Vec<String>,
}

/// Detailseite eines einzelnen Mods parsen — schlichte `<table><tr><td><b>Label</b></td>
/// <td>Wert</td></tr>…</table>` (Browser-DevTools live geprüft, 2026-08-07), Werte dort
/// **immer ungekürzt**, anders als in der Liste auf `mods.html`.
///
/// Gibt es Issues, folgt auf die `Issues`-Zeile (Zellwert = Anzahl) eine **weitere**, unbeschriftete
/// Zeile mit `<td colspan="2">` — jede Problemzeile durch einen echten Zeilenumbruch (nicht nur
/// `<br>`) im Quelltext getrennt, live geprüft. Ohne Issues fehlt diese Zeile ganz.
pub(crate) fn parse_mod_detail(html: &str) -> ModDetail {
    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("table tr").unwrap();
    let label_sel = Selector::parse("td b").unwrap();
    let td_sel = Selector::parse("td").unwrap();

    let mut fields: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut issues = Vec::new();
    for row in doc.select(&row_sel) {
        let Some(label_el) = row.select(&label_sel).next() else {
            continue;
        };
        let label = label_el.text().collect::<String>().trim().to_string();
        let Some(value_td) = row.select(&td_sel).nth(1) else {
            continue;
        };
        fields.insert(
            label.clone(),
            value_td.text().collect::<String>().trim().to_string(),
        );

        if label == "Issues" {
            let detail_row = row
                .next_siblings()
                .filter_map(ElementRef::wrap)
                .next()
                .filter(|r| r.select(&label_sel).next().is_none()); // keine eigene Feldzeile
            if let Some(detail_row) = detail_row {
                issues = detail_row
                    .text()
                    .collect::<String>()
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect();
            }
        }
    }

    ModDetail {
        display_name: fields.get("Name").cloned().and_then(non_empty),
        author: fields.get("Author").cloned().and_then(non_empty),
        version: fields.get("Version").cloned().and_then(non_empty),
        file_name: fields.get("Filename").cloned().and_then(non_empty),
        issues,
    }
}

/// Label/Wert-Paare einer `mods.html`-Zeile einlesen: jedes `div.col-lg-hidden` trägt ein
/// `<b>Label</b>`, das Geschwister mit Klasse `col-lg-12` (im selben Elternelement) den Wert.
fn row_columns(row: ElementRef, label_sel: &Selector) -> std::collections::HashMap<String, String> {
    let mut cols = std::collections::HashMap::new();
    for label_div in row.select(label_sel) {
        let label = label_div.text().collect::<String>().trim().to_string();
        if label.is_empty() {
            continue;
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

/// Name des Löschknopfs für eine Datei auf `mods.html` finden: `<button type="submit"
/// name="deleteactive|deleteinactive" value="<Datei>">`. Rückgabe ist der `name` (Formularfeld),
/// gepostet wird dann `<name>=<Datei>`. `None`, wenn kein Löschknopf existiert (Mod nicht da bzw.
/// Server läuft — dann fehlen die Knöpfe).
pub(crate) fn find_delete_button(html: &str, file_name: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(r#"button[type="submit"]"#).unwrap();
    doc.select(&sel)
        .find(|b| {
            b.value().attr("value") == Some(file_name)
                && b.value()
                    .attr("name")
                    .is_some_and(|n| n.starts_with("delete"))
        })
        .and_then(|b| b.value().attr("name").map(str::to_string))
}

/// Formularkörper zum Umschalten bauen: je Datei `<prefix><Datei>=on` (angehakte Checkbox),
/// dazu der Absende-Knopf `<submit_name>=<submit_value>` — genau wie der Browser sendet.
pub(crate) fn toggle_body(
    files: &[String],
    checkbox_prefix: &str,
    submit_name: &str,
    submit_value: &str,
) -> Vec<(String, String)> {
    let mut body: Vec<(String, String)> = files
        .iter()
        .map(|f| (format!("{checkbox_prefix}{f}"), "on".to_string()))
        .collect();
    body.push((submit_name.to_string(), submit_value.to_string()));
    body
}

/// „393.37 MB" → Bytes. `None`, wenn nicht als `<Zahl> <Einheit>` erkennbar.
pub(crate) fn parse_size(s: &str) -> Option<u64> {
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

    // Laufender Server: keine Checkboxen. Status kommt aus dem Abschnitt, Dateiname aus `id`
    // (aktiv) bzw. der Filename-Spalte (inaktiv). Echtes Markup, gekürzt.
    const ONLINE_HTML: &str = r#"
      <h2>Active Mods</h2>
      <div class="container-row col-md-visible col-xs-hidden grid-row"><div><b>Name</b></div></div>
      <div class="container-row grid-row modSelection-inactive" id="FS25_DashboardLive_VanillaVehicles.zip">
        <div class="col col-xs-9 col-md-12">Dashboard Live Vanilla Vehicles</div>
        <div class="col col-xs-9 col-md-12">1.0.0.0</div>
        <div class="col col-xs-9 col-md-12">Mister_mojo_AT</div>
        <div class="col col-xs-9 col-md-12">FS25_DashboardLive_VanillaVehicl...</div>
        <div class="col col-xs-9 col-md-12">393.37 MB</div>
      </div>
      <h2>Activate Mods</h2>
      <div class="container-row grid-row modSelection-inactive">
        <div class="col col-xs-9 col-md-12">Future Tech Production</div>
        <div class="col col-xs-9 col-md-12">2.1.0.0</div>
        <div class="col col-xs-9 col-md-12">emproLoop</div>
        <div class="col col-xs-9 col-md-12">FS25_crusher.zip</div>
        <div class="col col-xs-9 col-md-12">66.77 MB</div>
      </div>"#;

    #[test]
    fn laufender_server_status_aus_abschnitt() {
        let mods = parse_mods(ONLINE_HTML);
        assert_eq!(mods.len(), 2);
        // aktiv: Dateiname aus id (voll), Status aus Abschnitt „Active Mods"
        assert_eq!(mods[0].file_name, "FS25_DashboardLive_VanillaVehicles.zip");
        assert_eq!(mods[0].status, ModStatus::Active);
        assert_eq!(mods[0].version.as_deref(), Some("1.0.0.0"));
        // inaktiv: kein id/Checkbox → Dateiname aus Filename-Spalte, Status aus „Activate Mods"
        assert_eq!(mods[1].file_name, "FS25_crusher.zip");
        assert_eq!(mods[1].status, ModStatus::Inactive);
    }

    #[test]
    fn findet_loeschknopf_aktiv_und_inaktiv() {
        use super::find_delete_button;
        let html = r#"
          <form method="POST" action="mods.html?lang=en">
            <button type="submit" name="deleteactive" value="FS25_a.zip">Delete</button>
            <button type="submit" name="deleteinactive" value="b.dlc">Delete</button>
            <button type="button">Details</button>
          </form>"#;
        assert_eq!(
            find_delete_button(html, "FS25_a.zip").as_deref(),
            Some("deleteactive")
        );
        assert_eq!(
            find_delete_button(html, "b.dlc").as_deref(),
            Some("deleteinactive")
        );
        assert_eq!(find_delete_button(html, "gibtsnicht.zip"), None);
    }

    #[test]
    fn toggle_body_baut_checkboxen_und_absendeknopf() {
        let files = vec!["FS25_a.zip".to_string(), "b.dlc".to_string()];
        let body = super::toggle_body(&files, "moddeactivate_", "deactivate_mods", "Deactivate");
        assert_eq!(
            body,
            vec![
                ("moddeactivate_FS25_a.zip".to_string(), "on".to_string()),
                ("moddeactivate_b.dlc".to_string(), "on".to_string()),
                ("deactivate_mods".to_string(), "Deactivate".to_string()),
            ]
        );
    }

    // --- `mods.html`-Parser (echtes Markup, live per Browser-DevTools erfasst, 2026-08-07) ---

    use super::parse_mods_page_indexed;

    /// Testhilfe: wie [`parse_mods_page_indexed`], nur ohne die `mod_index`-Begleitinfo.
    fn parse_mods_page(html: &str) -> Vec<crate::model::ServerMod> {
        parse_mods_page_indexed(html)
            .into_iter()
            .map(|m| m.info)
            .collect()
    }

    // Eine inaktive Mod (Löschknopf `deleteinactive`) auf `mods.html`, Rest gekürzt.
    const MODS_PAGE_HTML: &str = r#"
      <div class="container-row col-lg-visible col-xs-hidden grid-row">
           <div class="col col-lg-2"><b>Name</b></div>
           <div class="col col-lg-1"><b>Version</b></div>
           <div class="col col-lg-2"><b>Author</b></div>
           <div class="col col-lg-3"><b>Filename</b></div>
           <div class="col col-lg-1"><b>Size</b></div>
           <div class="col col-lg-3 col-no-padding">
               <div class="container-row">
                   <div class="col col-lg-3"><b>Issues</b></div>
                   <div class="col col-lg-2"><b>Hub</b></div>
                   <div class="col col-lg-3"><b>Active</b></div>
               </div>
           </div>
      </div>
      <div class="container-row grid-row">
           <div class="container-row col col-lg-2 col-xs-12" title="Dashboard Live Vanilla Vehicles">
               <div class="col col-lg-hidden col-xs-3"><b>Name</b></div>
               <div class="col col-lg-12 col-xs-9"><i> <a href="mod.html?lang=en&amp;mod_index=0">Dashboard Live Vanilla Vehicles</a></i></div>
           </div>
           <div class="container-row col col-lg-1 col-xs-12" title="1.0.0.0">
               <div class="col col-lg-hidden col-xs-3"><b>Version</b></div>
               <div class="col col-lg-12 col-xs-9"><i><span>1.0.0.0</span></i>
                   <span title="An update is available for this mod."><img src="img/icons/updateIcon.png"></span>
               </div>
           </div>
           <div class="container-row col col-lg-2 col-xs-12" title="Mister_mojo_AT, SbSh">
               <div class="col col-lg-hidden col-xs-3"><b>Author</b></div>
               <div class="col col-lg-12 col-xs-9"><i><span>Mister_mojo_AT, ...</span></i></div>
           </div>
           <div class="container-row col col-lg-3 col-xs-12" title="FS25_DashboardLive_VanillaVehicles.zip">
               <div class="col col-lg-hidden col-xs-3"><b>Filename</b></div>
               <div class="col col-lg-12 col-xs-9"><i><a href="mod.html?lang=en&amp;mod_index=0">FS25_DashboardLive_VanillaVehicl...</a></i></div>
           </div>
           <div class="container-row col col-lg-1 col-xs-12" title="393.37 MB">
               <div class="col col-lg-hidden col-xs-3"><b>Size</b></div>
               <div class="col col-lg-12 col-xs-9"><i><span>393.37 MB</span></i></div>
           </div>
           <div class="container-row col col col-no-padding col-lg-3 col-xs-12">
               <div class="container-row col col-lg-3 col-xs-12" title="…">
                   <div class="col col-lg-hidden col-xs-3"><b>Issues</b></div>
                   <div class="col col-lg-12 col-xs-9"><i><a href="mod.html?lang=en&amp;mod_index=0">82</a></i></div>
               </div>
               <div class="container-row col col-lg-2 col-xs-12">
                   <div class="col col-lg-hidden col-xs-3"><b>ModHub</b></div>
                   <div class="col col-lg-12 col-xs-9"><i><span>Yes</span></i></div>
               </div>
               <div class="container-row col col-lg-3 col-xs-12">
                   <div class="col col-lg-hidden col-xs-3"><b>Active</b></div>
                   <div class="col col-lg-12 col-xs-9"><i><span>No</span></i></div>
               </div>
               <div class="container-row col col-lg-hidden col-xs-12">
                   <div class="col col-lg-hidden col-xs-2">
                       <a href="mods/FS25_DashboardLive_VanillaVehicles.zip"><img src="img/icons/saveIcon.png"></a>
                   </div>
                   <div class="col col-lg-hidden col-xs-2">
                       <button title="Delete FS25_DashboardLive_VanillaVehicles.zip" type="submit" name="deleteinactive" value="FS25_DashboardLive_VanillaVehicles.zip"><img src="img/icons/deleteIcon.png"></button>
                   </div>
               </div>
           </div>
      </div>
      <div class="container-row grid-row">
          <div class="container-row col col-lg-2 col-xs-12">
              <div class="col col-lg-hidden col-xs-3"><b>Name</b></div>
              <div class="col col-lg-12 col-xs-9"><i>SKY Agriculture Pack</i></div>
          </div>
          <div class="container-row col col-lg-1 col-xs-12">
              <div class="col col-lg-hidden col-xs-3"><b>Version</b></div>
              <div class="col col-lg-12 col-xs-9"><i>1.0.0.0</i></div>
          </div>
          <div class="container-row col col-lg-2 col-xs-12">
              <div class="col col-lg-hidden col-xs-3"><b>Author</b></div>
              <div class="col col-lg-12 col-xs-9"><i>GIANTS Software</i></div>
          </div>
      </div>"#;

    #[test]
    fn mods_html_liest_status_und_vollen_dateinamen_vom_loeschknopf() {
        let mods = parse_mods_page(MODS_PAGE_HTML);
        assert_eq!(
            mods.len(),
            1,
            "Katalogzeile ohne Filename-Spalte muss übersprungen werden"
        );
        let m = &mods[0];
        assert_eq!(m.file_name, "FS25_DashboardLive_VanillaVehicles.zip");
        assert_eq!(m.status, ModStatus::Inactive);
        assert_eq!(
            m.display_name.as_deref(),
            Some("Dashboard Live Vanilla Vehicles")
        );
        assert_eq!(m.version.as_deref(), Some("1.0.0.0"));
        assert_eq!(m.author.as_deref(), Some("Mister_mojo_AT, ..."));
        assert_eq!(m.size, Some((393.37 * 1024.0 * 1024.0) as u64));
        assert!(m.update_available);
        assert!(!m.is_dlc);
        assert!(m.from_modhub);
        assert_eq!(m.issue_count, 82);
    }

    #[test]
    fn mods_html_ohne_loeschknopf_faellt_auf_active_spalte_zurueck() {
        // Simuliert eine Zeile ohne Löschknopf (z. B. laufender Server) — Status/Dateiname
        // kommen dann aus der `Active`-Spalte bzw. der (ggf. gekürzten) Filename-Spalte.
        let html = r#"
          <div class="container-row grid-row">
               <div class="container-row col col-lg-3 col-xs-12">
                   <div class="col col-lg-hidden col-xs-3"><b>Filename</b></div>
                   <div class="col col-lg-12 col-xs-9"><i>FS25_crusher.zip</i></div>
               </div>
               <div class="container-row col col-lg-3 col-xs-12">
                   <div class="col col-lg-hidden col-xs-3"><b>Active</b></div>
                   <div class="col col-lg-12 col-xs-9"><i>Yes</i></div>
               </div>
          </div>"#;
        let mods = parse_mods_page(html);
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].file_name, "FS25_crusher.zip");
        assert_eq!(mods[0].status, ModStatus::Active);
    }

    #[test]
    fn mods_html_ohne_treffer_leere_liste() {
        assert!(parse_mods_page("<html><body>nichts</body></html>").is_empty());
    }

    // --- Nachladen gekürzter Felder von der Detailseite ---

    use super::{apply_detail, mod_index_from_href, needs_detail, parse_mod_detail, ModDetail};
    use crate::model::ServerMod;
    use scraper::{Html, Selector};

    #[test]
    fn liest_mod_index_aus_zeilen_link() {
        let mods = parse_mods_page_indexed(MODS_PAGE_HTML);
        assert_eq!(mods[0].mod_index, Some(0));
    }

    #[test]
    fn erkennt_gekuerzte_felder() {
        let mut m = ServerMod {
            file_name: "FS25_Kurz.zip".to_string(),
            display_name: Some("Voller Name".to_string()),
            version: None,
            author: Some("Autor Eins, Autor Zwei, ...".to_string()),
            size: None,
            is_dlc: false,
            status: ModStatus::Inactive,
            update_available: false,
            from_modhub: false,
            issue_count: 0,
            issues: Vec::new(),
        };
        assert!(needs_detail(&m)); // Autor gekürzt
        m.author = Some("Autor Eins".to_string());
        assert!(!needs_detail(&m));
        m.file_name = "FS25_Lang...".to_string();
        assert!(needs_detail(&m)); // Dateiname gekürzt
        m.file_name = "FS25_Kurz.zip".to_string();
        assert!(!needs_detail(&m));
        m.issue_count = 3;
        assert!(needs_detail(&m)); // Issues vorhanden → Volltext nur auf der Detailseite
        m.issue_count = 0;
        assert!(!needs_detail(&m));
        m.version = Some("1.2.3.4-Ultimate-...".to_string());
        assert!(needs_detail(&m)); // Version gekürzt
    }

    #[test]
    fn uebernimmt_nur_gekuerzte_felder_von_der_detailseite() {
        let mut m = ServerMod {
            file_name: "FS25_Voller_Name.zip".to_string(), // schon vollständig (vom Löschknopf)
            display_name: Some("Gekürzter Nam...".to_string()),
            version: Some("1.0.0.0".to_string()),
            author: Some("Autor A, ...".to_string()),
            size: Some(1024),
            is_dlc: false,
            status: ModStatus::Active,
            update_available: true,
            from_modhub: true,
            issue_count: 2,
            issues: Vec::new(),
        };
        apply_detail(
            &mut m,
            ModDetail {
                display_name: Some("Gekürzter Name, ausgeschrieben".to_string()),
                author: Some("Autor A, Autor B, Autor C".to_string()),
                version: Some("9.9.9.9".to_string()), // Version war nicht gekürzt → wird ignoriert
                file_name: Some("FS25_ANDERER_Name.zip".to_string()),
                issues: vec!["Erstes Problem".to_string(), "Zweites Problem".to_string()],
            },
        );
        assert_eq!(
            m.display_name.as_deref(),
            Some("Gekürzter Name, ausgeschrieben")
        );
        assert_eq!(m.author.as_deref(), Some("Autor A, Autor B, Autor C"));
        // Version war NICHT gekürzt → bleibt unverändert, wie beim Dateinamen.
        assert_eq!(m.version.as_deref(), Some("1.0.0.0"));
        // Dateiname war NICHT gekürzt → bleibt unverändert, auch wenn die Detailseite einen
        // anderen Wert liefert (schützt vor Vertauschung bei falschem mod_index).
        assert_eq!(m.file_name, "FS25_Voller_Name.zip");
        assert_eq!(m.issues, vec!["Erstes Problem", "Zweites Problem"]);
        // Unbeteiligte Felder unverändert.
        assert_eq!(m.size, Some(1024));
        assert_eq!(m.status, ModStatus::Active);
        assert!(m.from_modhub);
    }

    #[test]
    fn gekuerzte_version_wird_von_der_detailseite_ersetzt() {
        let mut m = ServerMod {
            file_name: "FS25_a.zip".to_string(),
            display_name: Some("Name".to_string()),
            version: Some("1.2.3.4-Ultimate-...".to_string()),
            author: Some("Autor".to_string()),
            size: None,
            is_dlc: false,
            status: ModStatus::Active,
            update_available: false,
            from_modhub: false,
            issue_count: 0,
            issues: Vec::new(),
        };
        apply_detail(
            &mut m,
            ModDetail {
                display_name: None,
                author: None,
                version: Some("1.2.3.4-Ultimate-Edition".to_string()),
                file_name: None,
                issues: Vec::new(),
            },
        );
        assert_eq!(m.version.as_deref(), Some("1.2.3.4-Ultimate-Edition"));
    }

    #[test]
    fn liest_detailseite_echtes_markup() {
        // Echtes Markup der Mod-Detailseite (mod.html?mod_index=…), live erfasst 2026-08-07.
        let html = r#"
          <div class="row"><h2>Mod</h2>
          <table><tbody>
            <tr><td><b>Name</b></td><td>Dashboard Live Vanilla Vehicles</td></tr>
            <tr><td><b>Version</b></td><td>1.0.0.0 <a href="mods.html?category=3&amp;lang=en"><img src="img/icons/updateIcon.png"></a></td></tr>
            <tr><td><b>Author</b></td><td>Mister_mojo_AT, SbSh, Glowins Modschmiede</td></tr>
            <tr><td><b>Filename</b></td><td><a href="mods/FS25_DashboardLive_VanillaVehicles.zip">FS25_DashboardLive_VanillaVehicles.zip</a></td></tr>
            <tr><td><b>Size</b></td><td>393.37 MB</td></tr>
            <tr><td><b>Active</b></td><td>No&nbsp;(<a href="index.html?lang=en#mods">» Mods can be activated in <b>HOME</b></a>)</td></tr>
          </tbody></table></div>"#;
        let detail = parse_mod_detail(html);
        assert_eq!(
            detail.display_name.as_deref(),
            Some("Dashboard Live Vanilla Vehicles")
        );
        assert_eq!(
            detail.author.as_deref(),
            Some("Mister_mojo_AT, SbSh, Glowins Modschmiede")
        );
        assert_eq!(detail.version.as_deref(), Some("1.0.0.0"));
        assert_eq!(
            detail.file_name.as_deref(),
            Some("FS25_DashboardLive_VanillaVehicles.zip")
        );
    }

    #[test]
    fn liest_issues_von_der_detailseite() {
        // Echtes Markup: nach der `Issues`-Zeile (Anzahl) folgt eine unbeschriftete Zeile mit
        // `colspan="2"`, Problemzeilen durch echte Zeilenumbrüche (nicht nur `<br>`) getrennt.
        let html = r#"
          <table><tbody>
            <tr><td><b>Name</b></td><td>Beispiel-Mod</td></tr>
            <tr><td><b>Issues</b></td><td>2</td></tr>
            <tr><td colspan="2">DDS texture file 'a.dds' is too big. Size 21.33 MB (max. 12.00 MB)
            <br>
            File count per store item too high. 226 found (max. 128)
            </td></tr>
          </tbody></table>"#;
        let detail = parse_mod_detail(html);
        assert_eq!(
            detail.issues,
            vec![
                "DDS texture file 'a.dds' is too big. Size 21.33 MB (max. 12.00 MB)".to_string(),
                "File count per store item too high. 226 found (max. 128)".to_string(),
            ]
        );
    }

    #[test]
    fn ohne_issues_zeile_bleibt_issues_liste_leer() {
        let html = r#"
          <table><tbody>
            <tr><td><b>Name</b></td><td>Beispiel-Mod</td></tr>
            <tr><td><b>Issues</b></td><td>0</td></tr>
            <tr><td><b>Map</b></td><td>No</td></tr>
          </tbody></table>"#;
        assert!(parse_mod_detail(html).issues.is_empty());
    }

    #[test]
    fn mod_index_aus_href_verschiedene_reihenfolgen() {
        let html = r#"<a href="mod.html?lang=en&mod_index=7">x</a>"#;
        let doc = Html::parse_document(html);
        let a = doc.select(&Selector::parse("a").unwrap()).next().unwrap();
        assert_eq!(mod_index_from_href(a), Some(7));

        let html2 = r#"<a href="mod.html?mod_index=12&lang=en">x</a>"#;
        let doc2 = Html::parse_document(html2);
        let a2 = doc2.select(&Selector::parse("a").unwrap()).next().unwrap();
        assert_eq!(mod_index_from_href(a2), Some(12));
    }
}
