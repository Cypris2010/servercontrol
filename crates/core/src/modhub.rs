//! ModHub: serverseitiger Download (`startmoddownload`, Fortschritt via
//! `modhubdownloadprogress`, beides in [`crate::session::Session`]) und Namenssuche über die
//! öffentliche ModHub-Website (Weg B, [`catalog`]). Siehe Pflichtenheft Kap. 4.4 / 10.
//!
//! **Zwei getrennte Hosts, zwei Parser** (Kap. 10): die Server-Seiten (`mods.html`, authentifiziert,
//! in `session.rs`) und die **öffentliche** Website `farming-simulator.com` (hier, ohne Login).
//! Beide liefern gerendertes HTML ohne API — Anker sind stabile Attribute/Klassen, keine
//! CSS-Grid-Reihenfolge (SZ1/Q4). Am lebenden Angebot verifiziert (Suche „weighing", `mod_id
//! 366506` → `FS25_weighingStations18m.zip`, deckungsgleich mit dem `startmoddownload`-Wert).

use scraper::{ElementRef, Html, Selector};

use crate::error::Error;
use crate::model::{CatalogDetails, CatalogEntry, ModhubCategoryEntry};
use crate::Result;

const MODHUB_HOST: &str = "https://www.farming-simulator.com";

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("ServerControl/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("reqwest-Client mit fester Konfiguration lässt sich immer bauen")
}

/// Öffentliche ModHub-Website (Weg B, Kap. 4.4 / 10.2 LH) — getrennt vom Server-Panel, weil
/// anderer Host und ohne Anmeldung.
pub mod catalog {
    use super::*;

    /// Namenssuche: `GET mods.php?title=fs2025&searchMod=<query>`. Der erste Treffer wäre
    /// „FEATURED MOD"-Werbung — die liegt strukturell außerhalb von `div.mod-item` und taucht
    /// hier daher gar nicht erst auf (kein manuelles Verwerfen nötig).
    ///
    /// Version/Dateiname stehen auf dieser Seite nicht drin (siehe [`CatalogEntry`]) — pro
    /// Treffer wird deshalb zusätzlich die Detailseite geladen, parallel statt nacheinander
    /// (die Suchergebnisseite selbst ist ohnehin nicht paginiert, bleibt also überschaubar
    /// viele Treffer). Schlägt das für einen einzelnen Treffer fehl, bleibt dessen
    /// Version/Dateiname einfach leer statt die ganze Suche abzubrechen.
    pub async fn search(query: &str) -> Result<Vec<CatalogEntry>> {
        let resp = client()
            .get(format!("{MODHUB_HOST}/mods.php"))
            .query(&[("title", "fs2025"), ("searchMod", query)])
            .send()
            .await
            .map_err(map_reqwest)?;
        let html = resp.text().await.map_err(map_reqwest)?;
        let mut entries = parse_search_results(&html)?;

        let details =
            futures_util::future::join_all(entries.iter().map(|e| details(e.mod_id))).await;
        for (entry, detail) in entries.iter_mut().zip(details) {
            match detail {
                Ok(d) => {
                    entry.version = d.version;
                    entry.file_name = d.file_name;
                }
                Err(e) => log::warn!(
                    "ModHub-Detailseite für mod_id {} fehlgeschlagen: {e}",
                    entry.mod_id
                ),
            }
        }
        Ok(entries)
    }

    /// Detailseite: `GET mod.php?mod_id=<id>&title=fs2025` — Version, Dateiname, Beschreibung.
    pub async fn details(mod_id: u64) -> Result<CatalogDetails> {
        let resp = client()
            .get(format!("{MODHUB_HOST}/mod.php"))
            .query(&[
                ("mod_id", mod_id.to_string()),
                ("title", "fs2025".to_string()),
            ])
            .send()
            .await
            .map_err(map_reqwest)?;
        let html = resp.text().await.map_err(map_reqwest)?;
        parse_details(&html, mod_id)
    }
}

/// Trefferkarten der Suchergebnisseite parsen: jede Karte ist ein `div.mod-item` mit
/// Vorschaubild+Link (`mod_id` aus der `href`), Name (`h4`), Autor (`p span`, „By: …") und
/// Bewertung (`.mod-item__rating-num`, z. B. „4.4 (55)").
fn parse_search_results(html: &str) -> Result<Vec<CatalogEntry>> {
    let doc = Html::parse_document(html);
    let card_sel = Selector::parse("div.mod-item").unwrap();
    let cards: Vec<ElementRef> = doc.select(&card_sel).collect();
    if cards.is_empty() && !html.contains("mod-search-input") {
        // Weder Treffer noch die Suchmaske selbst gefunden → Seite nicht wie erwartet
        // aufgebaut (GIANTS-Layoutänderung), nicht stillschweigend „keine Treffer" melden.
        log::warn!(
            "ModHub-Suchseite nicht erkannt, HTML-Anfang: {}",
            crate::session::html_excerpt(html)
        );
        return Err(Error::Parse(
            "ModHub-Suchseite nicht erkannt (Layout geändert?)".to_string(),
        ));
    }

    let link_sel = Selector::parse(r#"div.mod-item__img a[href*="mod_id="]"#).unwrap();
    let img_sel = Selector::parse("div.mod-item__img img").unwrap();
    let name_sel = Selector::parse("div.mod-item__content h4").unwrap();
    let author_sel = Selector::parse("div.mod-item__content p span").unwrap();
    let rating_sel = Selector::parse("div.mod-item__rating-num").unwrap();

    let mut entries = Vec::new();
    for card in cards {
        let Some(link) = card.select(&link_sel).next() else {
            continue;
        };
        let Some(mod_id) = link.value().attr("href").and_then(mod_id_from_href) else {
            continue;
        };
        let name = card
            .select(&name_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let author = card
            .select(&author_sel)
            .next()
            .map(|e| e.text().collect::<String>())
            .and_then(|s| s.trim().strip_prefix("By:").map(|s| s.trim().to_string()));
        let rating = card
            .select(&rating_sel)
            .next()
            .and_then(|e| rating_from_text(&e.text().collect::<String>()));
        let thumb_url = card
            .select(&img_sel)
            .next()
            .and_then(|e| e.value().attr("src"))
            .map(str::to_string);

        entries.push(CatalogEntry {
            mod_id,
            name,
            author,
            rating,
            thumb_url,
            version: None,
            file_name: None,
        });
    }
    Ok(entries)
}

/// Serverseitige ModHub-Kategorieseite parsen (`mods.html?category=<id>&lang=en`, Kap. 10.1
/// LH) — **nur bei gestopptem Server** aufrufbar (das prüft der Aufrufer in `session.rs`, hier
/// nur das Parsen). Live verifiziert: jeder Eintrag ist ein `div.grid-row` mit einem
/// `button[name="startmoddownload"]` (liefert `mod_id` aus `value` **und** den Dateinamen aus
/// `title="Install <Datei>"` — stabiler als die gekürzte Filename-Spalte). Anders als von
/// Kap. 10.1 vermutet stehen Name/Version/Autor/Größe **nicht** als Label/Wert-Paare, sondern
/// in derselben `div.col-lg-12.col-xs-9`-Wertspalten-Struktur wie die installierten Mods
/// (`mods.rs`) — nur mit `col-lg`- statt `col-md`-Klassen. Wir ankern deshalb direkt am
/// Download-Knopf statt an der Formular-`action` (funktioniert unabhängig von der Kategorie-ID
/// in der URL, robuster gegen Layoutdetails).
pub(crate) fn parse_category(html: &str) -> Result<Vec<ModhubCategoryEntry>> {
    let doc = Html::parse_document(html);
    let select_sel = Selector::parse("select#selectCategory").unwrap();
    if doc.select(&select_sel).next().is_none() {
        log::warn!(
            "ModHub-Kategorieseite nicht erkannt, HTML-Anfang: {}",
            crate::session::html_excerpt(html)
        );
        return Err(Error::Parse(
            "ModHub-Kategorieseite nicht erkannt (Server läuft oder Layout geändert?)".to_string(),
        ));
    }

    let row_sel = Selector::parse("div.grid-row").unwrap();
    let btn_sel = Selector::parse(r#"button[name="startmoddownload"]"#).unwrap();
    let value_sel = Selector::parse(".col-lg-12.col-xs-9").unwrap();

    let mut entries = Vec::new();
    for row in doc.select(&row_sel) {
        let Some(btn) = row.select(&btn_sel).next() else {
            continue;
        };
        let Some(mod_id) = btn.value().attr("value").and_then(|v| v.parse().ok()) else {
            continue;
        };
        let file_name = btn
            .value()
            .attr("title")
            .and_then(|t| t.strip_prefix("Install "))
            .unwrap_or_default()
            .to_string();
        let vals: Vec<String> = row
            .select(&value_sel)
            .map(|e| e.text().collect::<String>().trim().to_string())
            .collect();
        let col = |i: usize| vals.get(i).filter(|s| !s.is_empty()).cloned();

        entries.push(ModhubCategoryEntry {
            mod_id,
            name: col(0).unwrap_or_default(),
            version: col(1),
            author: col(2),
            file_name,
            size: vals.get(4).and_then(|s| crate::mods::parse_size(s)),
        });
    }
    Ok(entries)
}

/// Detailseite parsen: `h2.title-label` liefert den Namen, die Infotabelle
/// (`div.table-row` aus `<b>Label</b>` + Wertzelle) Autor/Dateiname/Version.
fn parse_details(html: &str, mod_id: u64) -> Result<CatalogDetails> {
    let doc = Html::parse_document(html);
    let name_sel = Selector::parse("h2.title-label").unwrap();
    let name = doc
        .select(&name_sel)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string())
        .ok_or_else(|| {
            log::warn!(
                "ModHub-Detailseite nicht erkannt, HTML-Anfang: {}",
                crate::session::html_excerpt(html)
            );
            Error::Parse("ModHub-Detailseite nicht erkannt (Layout geändert?)".to_string())
        })?;

    let row_sel = Selector::parse("div.table-row").unwrap();
    let label_sel = Selector::parse(".table-cell b").unwrap();
    let value_sel = Selector::parse(".table-cell").unwrap();
    let mut author = None;
    let mut file_name = None;
    let mut version = None;
    for row in doc.select(&row_sel) {
        if row.select(&label_sel).next().is_none() {
            continue;
        }
        let label = row
            .select(&label_sel)
            .next()
            .unwrap()
            .text()
            .collect::<String>();
        let label = label.trim().to_string();
        let value = row
            .select(&value_sel)
            .nth(1)
            .map(|e| e.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());
        match label.as_str() {
            "Author" => author = value,
            "Filename" => file_name = value,
            "Version" => version = value,
            _ => {}
        }
    }

    let desc_sel = Selector::parse("div.box-mods-item-info div.top-line").unwrap();
    let description = doc
        .select(&desc_sel)
        .next()
        .map(|e| e.text().collect::<Vec<_>>().join("").trim().to_string())
        .filter(|s| !s.is_empty());

    Ok(CatalogDetails {
        mod_id,
        name,
        author,
        version,
        file_name,
        description,
    })
}

/// `mod_id` aus einer `href` wie `mod.php?mod_id=366506&title=fs2025` lesen.
fn mod_id_from_href(href: &str) -> Option<u64> {
    let (_, rest) = href.split_once("mod_id=")?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// „4.4&nbsp;(55)" (bzw. „4&nbsp;(12)") → `4.4`. Der Text vor dem ersten Leerraum (auch NBSP)
/// ist die Bewertung.
fn rating_from_text(text: &str) -> Option<f32> {
    let head: String = text.chars().take_while(|c| !c.is_whitespace()).collect();
    head.trim().parse().ok()
}

fn map_reqwest(e: reqwest::Error) -> Error {
    log::warn!("HTTP-Fehler bei {:?}: {e}", e.url());
    if e.is_connect() || e.is_timeout() {
        Error::Unreachable
    } else {
        Error::Network(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Echtes Markup der Suchergebnisseite (`mods.php?title=fs2025&searchMod=weighing`),
    // auf zwei Karten reduziert (live am 2026-07-27 verifiziert).
    const SEARCH_HTML: &str = r#"
      <div class="mod-search-box">
        <form method="GET" action="/mods.php?title=fs2025&searchMod=weighing">
          <input type="hidden" name="title" value="fs2025">
          <input type="text" name="searchMod" value="weighing" class="mod-search-input">
        </form>
      </div>
      <div class="dlc-featured dlc-featured--mods clearfix">
        <div class="dlc-featured__info text-right">
          <div class="preheader">FEATURED MOD</div>
          <h3 class="color-white">North Frisian 25</h3>
          <a href="mod.php?mod_id=317176&title=fs2025" class="button button-buy">MORE INFO</a>
        </div>
      </div>
      <div class="row">
        <div class="medium-6 large-3 columns">
          <div class="mod-item">
            <div class="mod-item__img">
              <a href="mod.php?mod_id=363274&title=fs2025"><img src="https://cdn32.giants-software.com/modHub/storage/00363274/iconBig.jpg"></a>
            </div>
            <div class="mod-item__content">
              <h4> Weighing Platform</h4>
              <p><span>By: D4rkfr34k</span></p>
              <div class="mods-rating clearfix"><span class="icon-star"></span></div>
              <div class="mod-item__rating-num">4.4&nbsp;(55)
              </div>
            </div>
            <a href="mod.php?mod_id=363274&title=fs2025" class="button button-buy">MORE INFO</a>
          </div>
        </div>
        <div class="medium-6 large-3 columns">
          <div class="mod-item">
            <div class="mod-item__img">
              <a href="mod.php?mod_id=366506&title=fs2025"><img src="https://cdn32.giants-software.com/modHub/storage/00366506/iconBig.jpg"></a>
            </div>
            <div class="mod-item__content">
              <h4> Weigh Station Pack</h4>
              <p><span>By: [Weekend Farmers] Westpfalz Modding</span></p>
              <div class="mods-rating clearfix"><span class="icon-star"></span></div>
              <div class="mod-item__rating-num">5&nbsp;(16)
              </div>
            </div>
            <a href="mod.php?mod_id=366506&title=fs2025" class="button button-buy">MORE INFO</a>
          </div>
        </div>
      </div>"#;

    #[test]
    fn liest_suchtreffer_und_verwirft_featured_mod() {
        let entries = parse_search_results(SEARCH_HTML).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.mod_id != 317176));
    }

    #[test]
    fn treffer_felder_korrekt() {
        let entries = parse_search_results(SEARCH_HTML).unwrap();
        let e = entries.iter().find(|e| e.mod_id == 366506).unwrap();
        assert_eq!(e.name, "Weigh Station Pack");
        assert_eq!(
            e.author.as_deref(),
            Some("[Weekend Farmers] Westpfalz Modding")
        );
        assert_eq!(e.rating, Some(5.0));
        assert!(e.thumb_url.as_deref().unwrap().contains("00366506"));
    }

    #[test]
    fn dezimale_bewertung() {
        let entries = parse_search_results(SEARCH_HTML).unwrap();
        let e = entries.iter().find(|e| e.mod_id == 363274).unwrap();
        assert_eq!(e.rating, Some(4.4));
    }

    #[test]
    fn unerkannte_seite_ist_parse_fehler() {
        assert!(parse_search_results("<html><body>nichts</body></html>").is_err());
    }

    // Echtes Markup der Detailseite (`mod.php?mod_id=366506&title=fs2025`), gekürzt.
    const DETAIL_HTML: &str = r#"
      <div class="row box-mods-item-info">
        <h2 class="column title-label">Weigh Station Pack</h2>
        <div class="medium-12 large-8 columns">
          <div class="top-line">
            A pack of 6 ground-level weigh stations.<br />
            - Category: Buildings &gt; Tools<br />
          </div>
        </div>
        <div class="medium-6 large-4 columns">
          <div class="top-line">
            <div class="table table-game-info">
              <div class="table-row">
                <div class="table-cell"><b>Game</b></div>
                <div class="table-cell">Farming Simulator 25</div>
              </div>
              <div class="table-row">
                <div class="table-cell"><b>Author</b></div>
                <div class="table-cell"><a href="mods.php?org_id=237754">[Weekend Farmers] Westpfalz Modding</a></div>
              </div>
              <div class="table-row">
                <div class="table-cell"><b>Filename</b></div>
                <div class="table-cell">FS25_weighingStations18m.zip</div>
              </div>
              <div class="table-row">
                <div class="table-cell"><b>Size</b></div>
                <div class="table-cell">3.21 MB</div>
              </div>
              <div class="table-row">
                <div class="table-cell"><b>Version</b></div>
                <div class="table-cell">1.0.0.0</div>
              </div>
            </div>
          </div>
        </div>
      </div>"#;

    #[test]
    fn liest_detailseite() {
        let d = parse_details(DETAIL_HTML, 366506).unwrap();
        assert_eq!(d.mod_id, 366506);
        assert_eq!(d.name, "Weigh Station Pack");
        assert_eq!(
            d.author.as_deref(),
            Some("[Weekend Farmers] Westpfalz Modding")
        );
        assert_eq!(d.file_name.as_deref(), Some("FS25_weighingStations18m.zip"));
        assert_eq!(d.version.as_deref(), Some("1.0.0.0"));
        assert!(d.description.unwrap().contains("weigh stations"));
    }

    #[test]
    fn unerkannte_detailseite_ist_parse_fehler() {
        assert!(parse_details("<html><body>nichts</body></html>", 1).is_err());
    }

    // Echtes Markup der Server-Kategorieseite (`mods.html?category=3&lang=en`, „Update"),
    // live am 2026-07-27 verifiziert — auf Auswahl-Dropdown, Kopfzeile und einen Eintrag gekürzt.
    const CATEGORY_HTML: &str = r#"
      <select id="selectCategory" class="narrow-select" onchange="navigateCategory(this.value, 0)">
        <option id="optionCategory0" value="0">DLC</option>
        <option id="optionCategory1" value="1">All</option>
        <option id="optionCategory3" value="3" selected="">Update</option>
      </select>
      <form action="mods.html?category=3&amp;lang=en" method="POST">
        <div class="container table-grid table2">
          <div class="container-row col-lg-visible col-xs-hidden grid-row">
            <div class="col col-lg-3"><b>Name</b></div>
            <div class="col col-lg-1"><b>Version</b></div>
            <div class="col col-lg-2"><b>Author</b></div>
            <div class="col col-lg-3"><b>Filename</b></div>
            <div class="col col-lg-1"><b>Size</b></div>
            <div class="col col-lg-1"><b>Deps</b></div>
            <div class="col col-lg-1"></div>
          </div>
          <div class="container-row grid-row">
            <div class="container-row col col-lg-3 col-xs-12" title="Central Warehouse Pack">
              <div class="col col-lg-hidden col-xs-3"><b>Name</b></div>
              <div class="col col-lg-12 col-xs-9"><i>Central Warehouse Pack</i></div>
            </div>
            <div class="container-row col col-lg-1 col-xs-12" title="1.0.0.0">
              <div class="col col-lg-hidden col-xs-3"><b>Version</b></div>
              <div class="col col-lg-12 col-xs-9"><i>1.0.0.0</i></div>
            </div>
            <div class="container-row col col-lg-2 col-xs-12" title="Kamikater">
              <div class="col col-lg-hidden col-xs-3"><b>Author</b></div>
              <div class="col col-lg-12 col-xs-9"><i>Kamikater</i></div>
            </div>
            <div class="container-row col col-lg-3 col-xs-12" title="FS25_CentralWarehousePack.zip">
              <div class="col col-lg-hidden col-xs-3"><b>Filename</b></div>
              <div class="col col-lg-12 col-xs-9"><i>FS25_CentralWarehousePack.zip</i></div>
            </div>
            <div class="container-row col col-lg-1 col-xs-12" title="9.91 MB">
              <div class="col col-lg-hidden col-xs-3"><b>Size</b></div>
              <div class="col col-lg-12 col-xs-9"><i>9.91 MB</i></div>
            </div>
            <div class="container-row col col-lg-1 col-xs-12" title="">
              <div class="col col-lg-hidden col-xs-3"><b>Deps</b></div>
              <div class="col col-lg-12 col-xs-9">         </div>
            </div>
            <div class="container-row col-lg-1 col-xs-12">
              <div class="col col-xs-4 col-lg-12 text-center">
                <button title="Install FS25_CentralWarehousePack.zip" type="submit" name="startmoddownload" value="309712"><img class="icon" src="img/icons/downloadIcon.png"></button>
              </div>
            </div>
          </div>
        </div>
      </form>"#;

    #[test]
    fn liest_kategorieseite() {
        let entries = parse_category(CATEGORY_HTML).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.mod_id, 309712);
        assert_eq!(e.name, "Central Warehouse Pack");
        assert_eq!(e.version.as_deref(), Some("1.0.0.0"));
        assert_eq!(e.author.as_deref(), Some("Kamikater"));
        assert_eq!(e.file_name, "FS25_CentralWarehousePack.zip");
        assert_eq!(e.size, Some((9.91 * 1024.0 * 1024.0) as u64));
    }

    #[test]
    fn unerkannte_kategorieseite_ist_parse_fehler() {
        assert!(parse_category("<html><body>nichts</body></html>").is_err());
    }

    #[test]
    fn mod_id_aus_href() {
        assert_eq!(
            mod_id_from_href("mod.php?mod_id=366506&title=fs2025"),
            Some(366506)
        );
        assert_eq!(mod_id_from_href("mod.php?title=fs2025"), None);
    }
}
