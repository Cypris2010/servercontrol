//! Anmeldung, Cookie-Sitzung und reaktiver Re-Login (Pflichtenheft Kap. 6).
//!
//! **Die kritische Eigenheit des FS-Panels (verifiziert): der `Cookie`-Header wird
//! case-sensitiv geprüft** — der Server erkennt den Sitzungs-Cookie nur bei exakt `Cookie:`
//! (großes C). reqwest/hyper schreibt Header-Namen sonst klein (`cookie:`), was der Server
//! komplett ignoriert; jede Anfrage wirkt dann wie eine neue, nicht angemeldete Sitzung und
//! die Anmeldung greift nie. Deshalb `http1_title_case_headers()`.
//!
//! Das Cookie wird **selbst geführt** statt über reqwests eingebauten Speicher: Der Server
//! schickt in jeder Antwort ein `Set-Cookie`; solange der Cookie (dank Title-Case) erkannt
//! wird, bleibt die `SessionID` stabil (verifiziert). Beim Login *könnte* der Server sie
//! wechseln (Session-Fixation-Schutz) — nicht sauber isoliert nachweisbar, aber wir folgen
//! dem Cookie nach jeder Antwort, damit die Sitzung in **beiden** Fällen robust weiterläuft.
//! Ein `User-Agent` wird gesetzt (reqwest sendet per Default keinen); für die Sitzung ist er
//! nicht erforderlich (verifiziert), aber er kennzeichnet den Client sauber.

use std::sync::Mutex;

use reqwest::header::COOKIE;
use reqwest::{Client, RequestBuilder, Response};
use scraper::{Html, Selector};
use url::Url;

use crate::error::Error;
use crate::model::ServerState;
use crate::secret::Secret;
use crate::Result;

/// Name des Sitzungs-Cookies im FS-Panel.
const SESSION_COOKIE: &str = "SessionID";

/// Authentifizierte HTTP-Sitzung gegen ein FS25-Web-Panel.
///
/// `username`/`password` bleiben für den **reaktiven Re-Login** erhalten: Läuft die Sitzung
/// ab, meldet sich die Bibliothek einmal transparent neu an (folgende Schritte).
pub(crate) struct Session {
    client: Client,
    base_url: Url,
    username: String,
    password: Secret,
    /// Aktuelle SessionID; folgt dem Cookie (nach jeder Antwort aus `Set-Cookie` nachgezogen).
    session_id: Mutex<String>,
}

impl Session {
    /// GET Login-Seite → POST Zugangsdaten (Cookie folgt der Rotation) → Erfolg an frischem GET belegen.
    ///
    /// Kommt trotz POST wieder das Login-Formular zurück, gelten die Zugangsdaten als
    /// abgelehnt (`AuthFailed`). Host/Panel nicht erreichbar → `Unreachable`.
    pub(crate) async fn login(
        base_url: Url,
        accept_invalid_cert: bool,
        username: String,
        password: Secret,
    ) -> Result<Self> {
        // **Title-Case-Header zwingend (verifiziert):** Das FS-Panel prüft Header-Namen
        // case-sensitiv und erkennt den Sitzungs-Cookie nur als `Cookie` (großes C). reqwest
        // schreibt sonst alle Namen klein (`cookie`) → der Server ignoriert ihn, die Sitzung
        // greift nie. User-Agent nur zur Client-Kennzeichnung (reqwest sendet sonst keinen).
        // Cookie führen wir selbst (siehe Modulkopf), daher kein `cookie_store`.
        let client = Client::builder()
            .user_agent(concat!("ServerControl/", env!("CARGO_PKG_VERSION")))
            .http1_title_case_headers()
            // Kein Keep-Alive-Pooling: Das FS-Panel schließt Verbindungen nach jeder Antwort;
            // eine wiederverwendete Pool-Verbindung ist dann tot und der nächste Request stirbt
            // mit „error sending request". Jede Anfrage frisch (wie curl) — verifiziert.
            .pool_max_idle_per_host(0)
            .danger_accept_invalid_certs(accept_invalid_cert)
            .build()
            .map_err(|e| Error::Network(e.to_string()))?;

        let session = Self {
            client,
            base_url,
            username,
            password,
            session_id: Mutex::new(String::new()),
        };
        session.authenticate().await?;
        Ok(session)
    }

    /// Zugangsdaten posten und den Erfolg an einem frischen GET prüfen.
    async fn authenticate(&self) -> Result<()> {
        // 1) Login-Seite holen (setzt die erste SessionID) und die echte `action` auslesen
        //    (sie trägt `?lang=…`; darauf reagiert der Login-Handler).
        let login_page = self
            .send(self.client.get(self.base_url.clone()))
            .await?
            .text()
            .await
            .map_err(map_reqwest)?;
        let action = login_form_action(&login_page)
            .and_then(|a| self.base_url.join(&a).ok())
            .unwrap_or_else(|| self.base_url.clone());

        // 2) Zugangsdaten absetzen. Feldreihenfolge wie im Browser: username, password,
        //    Absende-Knopf `login=Login`. `password` verlässt den `Secret` nur hier (Q1).
        //    Falls der Server die SessionID beim Login wechselt, zieht `send` sie automatisch
        //    aus `Set-Cookie` nach.
        let form = [
            ("username", self.username.as_str()),
            ("password", self.password.expose()),
            ("login", "Login"),
        ];
        self.send(self.client.post(action).form(&form)).await?;

        // 3) Erfolg an einem frischen GET mit der (rotierten) SessionID belegen: kommt jetzt
        //    noch das Login-Formular, wurden die Zugangsdaten abgelehnt.
        let verify = self
            .send(self.client.get(self.base_url.clone()))
            .await?
            .text()
            .await
            .map_err(map_reqwest)?;
        if is_login_page(&verify) {
            return Err(Error::AuthFailed);
        }
        Ok(())
    }

    /// Aktueller Laufzeitzustand (online/offline + Spielversion), Pflichtenheft 9.1.
    pub(crate) async fn state(&self) -> Result<ServerState> {
        parse_state(&self.home().await?)
    }

    /// Aktive und inaktive Mods lesen (Pflichtenheft 10.5 LH).
    pub(crate) async fn list_mods(&self) -> Result<Vec<crate::model::ServerMod>> {
        Ok(crate::mods::parse_mods(&self.home().await?))
    }

    /// Authentifizierten GET der Home-Seite; erneuert die Sitzung **einmal transparent**, falls
    /// sie abgelaufen ist (Login-Formular zurück statt Home) — reaktiv, kein Pollen (Kap. 6).
    async fn home(&self) -> Result<String> {
        let body = self.get_text(self.base_url.clone()).await?;
        if !is_login_page(&body) {
            return Ok(body);
        }
        // Sitzung abgelaufen → einmal neu anmelden und Home erneut holen.
        self.authenticate().await?;
        let body = self.get_text(self.base_url.clone()).await?;
        if is_login_page(&body) {
            return Err(Error::AuthFailed);
        }
        Ok(body)
    }

    /// GET einer URL mit der Sitzung, Antwort als Text.
    async fn get_text(&self, url: Url) -> Result<String> {
        self.send(self.client.get(url))
            .await?
            .text()
            .await
            .map_err(map_reqwest)
    }

    /// Abmelden: GET `index.html?logout=true` (Kap. 6).
    pub(crate) async fn logout(&self) -> Result<()> {
        let mut url = self.base_url.clone();
        url.set_query(Some("logout=true"));
        self.send(self.client.get(url)).await?;
        Ok(())
    }

    /// Request mit aktueller SessionID abschicken und dem Cookie folgen: hängt den `Cookie`
    /// an (sofern schon einer bekannt ist) und übernimmt danach eine ggf. geänderte ID aus
    /// `Set-Cookie`. So läuft die Sitzung wie ein Browser-Jar weiter, auch falls der Server
    /// die ID beim Login wechselt.
    async fn send(&self, req: RequestBuilder) -> Result<Response> {
        let sid = self.session_id.lock().unwrap().clone();
        let req = if sid.is_empty() {
            req
        } else {
            req.header(COOKIE, format!("{SESSION_COOKIE}={sid}"))
        };
        let resp = req.send().await.map_err(map_reqwest)?;
        if let Some(new_sid) = session_id(&resp) {
            *self.session_id.lock().unwrap() = new_sid;
        }
        Ok(resp)
    }
}

/// Ist die zurückgelieferte Seite das Login-Formular? Erkennungsmerkmal (Kap. 6):
/// `form[name="input"]` **und** ein Passwortfeld. Dient (a) der Abweisung falscher
/// Zugangsdaten und (b) später dem Erkennen einer abgelaufenen Sitzung.
fn is_login_page(html: &str) -> bool {
    let doc = Html::parse_document(html);
    // `unwrap` ist hier unkritisch: die Selektoren sind konstante, gültige Literale.
    let form = Selector::parse(r#"form[name="input"]"#).unwrap();
    let password = Selector::parse(r#"input[type="password"]"#).unwrap();
    doc.select(&form).next().is_some() && doc.select(&password).next().is_some()
}

/// Laufzeitzustand aus der Home-Seite lesen (Pflichtenheft 9.1). Primärer Anker:
/// `div.status-indicator` mit Modifier-Klasse `online`/`offline`. Online zusätzlich die
/// Spielversion (best effort).
fn parse_state(html: &str) -> Result<ServerState> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("div.status-indicator").unwrap();
    let class = doc
        .select(&sel)
        .next()
        .and_then(|e| e.value().attr("class"))
        .ok_or_else(|| Error::Parse("status-indicator nicht gefunden".to_string()))?;
    let has = |name: &str| class.split_whitespace().any(|c| c == name);
    if has("online") {
        Ok(ServerState::Online {
            version: parse_game_version(html),
        })
    } else if has("offline") {
        Ok(ServerState::Offline)
    } else {
        Err(Error::Parse(
            "status-indicator ohne online/offline".to_string(),
        ))
    }
}

/// Spielversion aus der eingeloggten Home-Seite: die „Game"-Zeile trägt z. B.
/// `Farming Simulator 25 (1.19.0.0)` → `1.19.0.0`. **Nicht** der Web-Interface-Build aus dem
/// Footer (`10.0.0.0`). `None`, wenn nicht gefunden (z. B. offline).
fn parse_game_version(html: &str) -> Option<String> {
    // Die Seite enthält „Farming Simulator" mehrfach (u. a. im `<title>`). Gesucht ist die
    // „Game"-Zeile `Farming Simulator 25 (1.19.0.0)`, wo die Klammer **dicht** folgt — daran
    // grenzen wir sie vom Titel („… Dedicated Server") ab.
    for (i, _) in html.match_indices("Farming Simulator") {
        let rest = &html[i..];
        let Some(open) = rest.find('(') else { continue };
        if open > 30 {
            continue; // Klammer zu weit weg → nicht die Versionszeile
        }
        let Some(close_rel) = rest[open..].find(')') else {
            continue;
        };
        let v = rest[open + 1..open + close_rel].trim();
        if v.contains('.') && v.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return Some(v.to_string());
        }
    }
    None
}

/// SessionID-Cookie aus einer Antwort lesen.
fn session_id(resp: &Response) -> Option<String> {
    resp.cookies()
        .find(|c| c.name() == SESSION_COOKIE)
        .map(|c| c.value().to_string())
}

/// `action`-Ziel des Login-Formulars (`form[name="input"]`) auslesen, z. B.
/// `index.html?lang=en`. Der Login-Handler reagiert nur auf diese vollständige URL.
fn login_form_action(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let form = Selector::parse(r#"form[name="input"]"#).unwrap();
    doc.select(&form)
        .next()
        .and_then(|f| f.value().attr("action"))
        .map(str::to_string)
}

/// reqwest-Fehler auf unsere Fehlerfälle abbilden: Verbindungs-/Zeitfehler → `Unreachable`,
/// alles andere → `Network` (mit Grund, aber ohne Zugangsdaten).
fn map_reqwest(e: reqwest::Error) -> Error {
    if e.is_connect() || e.is_timeout() {
        Error::Unreachable
    } else {
        Error::Network(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::is_login_page;

    #[test]
    fn erkennt_login_formular() {
        let html = r#"<html><body>
            <form name="input" action="index.html?lang=en" method="post">
              <input type="text" name="username">
              <input type="password" name="password">
              <input type="submit" name="login" value="Login">
            </form></body></html>"#;
        assert!(is_login_page(html));
    }

    #[test]
    fn liest_action_aus_login_formular() {
        // Echtes Markup des FS25-Panels (Attribute in Originalreihenfolge/-schreibweise).
        let html = r#"<form name="input" action="index.html?lang=en" method="POST">
            <input type="text" name="username" value="">
            <input type="password" name="password" value="">
            <input class="button" name="login" type="submit" value="Login"></form>"#;
        assert_eq!(
            super::login_form_action(html).as_deref(),
            Some("index.html?lang=en")
        );
    }

    #[test]
    fn home_seite_ist_kein_login() {
        let html = r#"<html><body>
            <div class="status-indicator offline"><span>OFFLINE</span></div>
            <form name="configuration"></form></body></html>"#;
        assert!(!is_login_page(html));
    }

    // --- Zustandserkennung (echtes Markup vom FS25-Panel, Umgebungsdaten anonymisiert) ---

    #[test]
    fn erkennt_online_mit_version() {
        // Mit `<title>` (enthält ebenfalls „Farming Simulator") — der Parser darf trotzdem die
        // Versionszeile treffen, nicht den Titel.
        let html = r#"<title>Farming Simulator Dedicated Server | ONLINE</title><header>
            <div class="status-indicator online"><span>ONLINE</span></div></header>
            <form name="configuration" action="index.html?lang=en" method="POST">
              <div class="row column table-even">
                <div class="medium-3 columns column-label">Game</div>
                <div class="medium-9 columns">Farming Simulator 25 (1.19.0.0)</div>
              </div>
            </form>
            <a href="http://www.farming-simulator.com">10.0.0.0</a>"#;
        assert_eq!(
            super::parse_state(html).unwrap(),
            super::ServerState::Online {
                version: Some("1.19.0.0".to_string())
            }
        );
    }

    #[test]
    fn erkennt_offline() {
        let html = r#"<header>
            <div class="status-indicator offline"><span>OFFLINE</span></div></header>"#;
        assert_eq!(
            super::parse_state(html).unwrap(),
            super::ServerState::Offline
        );
    }

    #[test]
    fn version_nimmt_nicht_den_footer_build() {
        // Ohne „Game"-Zeile (z. B. Markup-Änderung) → lieber keine Version als der 10.0.0.0-Build.
        let html = r#"<div class="status-indicator online"><span>ONLINE</span></div>
            <a href="http://www.farming-simulator.com">10.0.0.0</a>"#;
        assert_eq!(
            super::parse_state(html).unwrap(),
            super::ServerState::Online { version: None }
        );
    }

    #[test]
    fn ohne_status_indicator_ist_parse_fehler() {
        assert!(super::parse_state("<html><body>nix</body></html>").is_err());
    }
}
