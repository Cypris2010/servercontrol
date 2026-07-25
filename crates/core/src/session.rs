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
}
