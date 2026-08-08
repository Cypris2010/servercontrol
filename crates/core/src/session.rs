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
//!
//! **Seit Patch 21 (verifiziert): der Login-POST antwortet mit `303 See Other`** (Post/
//! Redirect/Get) statt direkt mit der Seite. reqwest folgt Redirects standardmäßig selbst,
//! entfernt dabei aber unseren manuell gesetzten `Cookie`-Header (Schutz gegen Cookie-Leaks
//! bei Redirects) — die automatisch gefolgte Anfrage geht dann unauthentifiziert raus und
//! landet wieder auf der Login-Seite (`AuthFailed` trotz korrekter Zugangsdaten). Deshalb
//! `redirect::Policy::none()` und Redirects selbst mit unserem Cookie verfolgen (`send_raw`).

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::header::COOKIE;
use reqwest::{Client, RequestBuilder, Response};
use scraper::{Html, Selector};
use url::Url;

use crate::error::Error;
use crate::model::{ModStatus, ServerState};
use crate::secret::Secret;
use crate::Result;

/// Name des Sitzungs-Cookies im FS-Panel.
const SESSION_COOKIE: &str = "SessionID";

/// Obergrenze für selbst verfolgte Redirects (Post/Redirect/Get u. ä.) — schützt vor
/// Redirect-Schleifen, weit über dem, was das Panel je verkettet.
const MAX_REDIRECTS: u8 = 10;

/// Poll-Intervall beim Warten auf einen Zustandswechsel (start/stop/restart).
const POLL_INTERVAL: Duration = Duration::from_secs(3);
/// Zeitgrenze fürs Hochfahren — großzügig, da viele Mods das Laden verlängern.
const START_TIMEOUT: Duration = Duration::from_secs(300);
/// Zeitgrenze fürs Herunterfahren.
const STOP_TIMEOUT: Duration = Duration::from_secs(90);

/// Zeitgrenze für einen Live-Log-Long-Poll. Kommen bis dahin keine neuen Zeilen, kehrt
/// `read_log` mit leerem Abschnitt zurück (der Aufrufer pollt einfach weiter).
const LONGPOLL_TIMEOUT: Duration = Duration::from_secs(45);

/// Web-Upload-Grenze des Panels („Maximal allowed upload size is 1.71 GB"). Konservativ als
/// dezimale GB angesetzt; größere Dateien brauchen FTP/SFTP (`NoFileAccess`, MZ4).
const MAX_WEB_UPLOAD: u64 = 1_710_000_000;

/// Poll-Intervall für den ModHub-Downloadfortschritt — wie das Panel selbst (`frontend.js`,
/// Kap. 7.7 LH: 1-Sekunden-Takt).
const MODHUB_POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Zeitgrenze für einen ModHub-Download — großzügig für große Mods/Maps über langsame Leitungen.
const MODHUB_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(1800);

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
            // Redirects selbst verfolgen (siehe Modulkopf, Patch-21-Kommentar): reqwests
            // eingebaute Redirect-Behandlung entfernt unseren manuellen `Cookie`-Header.
            .redirect(reqwest::redirect::Policy::none())
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
        let action = form_action(&login_page, "input")
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

    /// Aktive und inaktive Mods lesen (Pflichtenheft 10.5 LH). Quelle ist primär **`mods.html`**
    /// (Browser-DevTools live geprüft, 2026-08-07): dort trägt jede Zeile eine explizite
    /// `Active`-Spalte **und** — über den Löschknopf — den vollen, ungekürzten Dateinamen; die
    /// Home-Seite liefert den vollen Namen dagegen nur für aktive Mods (über die Checkbox), bei
    /// inaktiven im laufenden Server nur die ggf. gekürzte Filename-Spalte.
    ///
    /// **Fallback auf die Home-Seite**, falls `mods.html` keine Mods liefert: Das Panel zeigt
    /// dort bei gestopptem Server zusätzlich Löschknöpfe, das Verhalten bei laufendem Server ist
    /// nicht live verifiziert — die Home-Seite ist es (`laufender_server_status_aus_abschnitt`).
    /// So bleibt die Liste in jedem Fall gefüllt, auch falls `mods.html` sich im laufenden
    /// Zustand anders verhält als bisher angenommen.
    ///
    /// **Gekürzte Felder** (Name/Autor/Dateiname enden auf „…", z. B. lange ModHub-Titel) werden
    /// von der Mod-Detailseite (`mod.html?mod_index=<i>`) nachgeladen — dort stehen sie immer
    /// ungekürzt (live geprüft). Pro betroffener Mod **ein** zusätzlicher Request, unabhängig
    /// davon, wie viele ihrer Felder gekürzt sind (nicht pro Feld); Mods ohne gekürztes Feld
    /// verursachen keinen zusätzlichen Request.
    pub(crate) async fn list_mods(&self) -> Result<Vec<crate::model::ServerMod>> {
        let mods_url = self.mods_page_url()?;
        let parsed = crate::mods::parse_mods_page_indexed(&self.get_authenticated(mods_url).await?);
        if parsed.is_empty() {
            return Ok(crate::mods::parse_mods(&self.home().await?));
        }

        let mut mods = Vec::with_capacity(parsed.len());
        for entry in parsed {
            let mut info = entry.info;
            if crate::mods::needs_detail(&info) {
                if let Some(mod_index) = entry.mod_index {
                    if let Ok(html) = self.get_authenticated(self.mod_detail_url(mod_index)?).await {
                        crate::mods::apply_detail(&mut info, crate::mods::parse_mod_detail(&html));
                    }
                }
            }
            mods.push(info);
        }
        Ok(mods)
    }

    /// URL der Mod-Detailseite (`mod.html?mod_index=<i>&lang=en`) — trägt Name/Autor/Dateiname
    /// ungekürzt, anders als die Listenansicht auf `mods.html`.
    fn mod_detail_url(&self, mod_index: u32) -> Result<Url> {
        self.base_url
            .join(&format!("mod.html?mod_index={mod_index}&lang=en"))
            .map_err(|e| Error::Parse(e.to_string()))
    }

    /// Einen Mod löschen — **nur bei gestopptem Server** (Kap. 7.3 LH). Auf `mods.html` trägt
    /// jeder Mod einen Löschknopf `deleteactive|deleteinactive=<Datei>`. Q3: danach muss der Mod
    /// aus der Liste verschwunden sein, sonst `NotProven`.
    pub(crate) async fn delete_mod(&self, file_name: &str) -> Result<()> {
        let mods_url = self.mods_page_url()?;
        let page = self.get_text(mods_url.clone()).await?;
        if matches!(parse_state(&page)?, ServerState::Online { .. }) {
            return Err(Error::ServerRunning);
        }
        let field = crate::mods::find_delete_button(&page, file_name).ok_or_else(|| {
            Error::FormMismatch(format!("Kein Löschknopf für {file_name} gefunden"))
        })?;
        self.send(
            self.client
                .post(mods_url)
                .form(&[(field.as_str(), file_name)]),
        )
        .await?;

        let gone = !self
            .list_mods()
            .await?
            .iter()
            .any(|m| m.file_name == file_name);
        if gone {
            Ok(())
        } else {
            Err(Error::NotProven)
        }
    }

    /// Einen Mod über das Web-Panel hochladen (`modUpload`, multipart). Bis 1,71 GB; darüber
    /// `NoFileAccess` (dann FTP/SFTP nötig, MZ4). Q3: danach muss der Mod in der Liste stehen.
    pub(crate) async fn upload_mod(&self, path: &Path) -> Result<()> {
        self.upload_mod_with_progress(path, |_| {}).await
    }

    /// Wie [`Self::upload_mod`], meldet aber laufend den Fortschritt (Kap. 7.3, `progress`-
    /// Events) — die Datei wird gestreamt statt komplett in den Speicher gelesen, damit auch
    /// Dateien nahe der 1,71-GB-Grenze den Prozess nicht aufblähen.
    pub(crate) async fn upload_mod_with_progress<F>(&self, path: &Path, on_progress: F) -> Result<()>
    where
        F: Fn(crate::model::Progress) + Send + Sync + 'static,
    {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::Parse("ungültiger Dateiname".to_string()))?
            .to_string();
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|e| Error::Parse(format!("Datei nicht lesbar: {e}")))?;
        let total = file
            .metadata()
            .await
            .map_err(|e| Error::Parse(format!("Datei nicht lesbar: {e}")))?
            .len();
        if total > MAX_WEB_UPLOAD {
            return Err(Error::NoFileAccess);
        }

        let sent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let stream = tokio_util::io::ReaderStream::new(file).inspect(move |chunk| {
            if let Ok(bytes) = chunk {
                let now = sent.fetch_add(bytes.len() as u64, std::sync::atomic::Ordering::Relaxed)
                    + bytes.len() as u64;
                on_progress(crate::model::Progress {
                    done: now,
                    total: Some(total),
                });
            }
        });
        let part = reqwest::multipart::Part::stream_with_length(
            reqwest::Body::wrap_stream(stream),
            total,
        )
        .file_name(file_name.clone())
        .mime_str("application/zip")
        .map_err(map_reqwest)?;
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("file_upload", "Upload");

        let url = self.mods_page_url()?;
        self.send(self.client.post(url).multipart(form)).await?;

        let present = self
            .list_mods()
            .await?
            .iter()
            .any(|m| m.file_name == file_name);
        if present {
            Ok(())
        } else {
            Err(Error::NotProven)
        }
    }

    /// ModHub-Download **durch den Server** auslösen: `startmoddownload=<mod_id>` auf einer
    /// Kategorieseite (Kategorie „All" = 1 reicht für jede numerische ID — die Seite dient nur
    /// als Formularziel, nicht als Filter). **Nur bei gestopptem Server** (Kap. 7.7 LH: ohne
    /// Login *oder* bei laufendem Server liefert `?category=` die installierten Mods statt des
    /// ModHub-Katalogs — keine `startmoddownload`-Buttons).
    pub(crate) async fn modhub_start(&self, mod_id: u64) -> Result<()> {
        if matches!(self.state().await?, ServerState::Online { .. }) {
            return Err(Error::ServerRunning);
        }
        let url = self
            .base_url
            .join("mods.html?category=1&lang=en")
            .map_err(|e| Error::Parse(e.to_string()))?;
        self.send(
            self.client
                .post(url)
                .form(&[("startmoddownload", mod_id.to_string())]),
        )
        .await?;
        Ok(())
    }

    /// Fortschritt eines laufenden ModHub-Downloads: `GET
    /// mods.html?modhubdownloadprogress=<mod_id>` → JSON `{downloaded, total}` (Kap. 7.7 LH).
    pub(crate) async fn modhub_progress(&self, mod_id: u64) -> Result<crate::model::Progress> {
        let url = self
            .base_url
            .join(&format!("mods.html?modhubdownloadprogress={mod_id}"))
            .map_err(|e| Error::Parse(e.to_string()))?;
        let text = self.get_text(url).await?;
        #[derive(serde::Deserialize)]
        struct Raw {
            downloaded: u64,
            total: u64,
        }
        let raw: Raw = serde_json::from_str(&text)
            .map_err(|e| Error::Parse(format!("modhubdownloadprogress: {e}")))?;
        Ok(crate::model::Progress {
            done: raw.downloaded,
            total: Some(raw.total),
        })
    }

    /// Laufenden ModHub-Download abbrechen (`cancelmoddownload=<mod_id>`).
    pub(crate) async fn modhub_cancel(&self, mod_id: u64) -> Result<()> {
        let url = self
            .base_url
            .join("mods.html?category=1&lang=en")
            .map_err(|e| Error::Parse(e.to_string()))?;
        self.send(
            self.client
                .post(url)
                .form(&[("cancelmoddownload", mod_id.to_string())]),
        )
        .await?;
        Ok(())
    }

    /// Bequemlichkeit: startet einen ModHub-Download, pollt bis `downloaded >= total` und
    /// **verifiziert** danach (Q3), dass `expected_file_name` in der Mod-Liste steht — sonst
    /// `NotProven`. `expected_file_name` liefert die GUI aus [`crate::modhub::catalog::details`]
    /// (ohnehin für die Anzeige geholt).
    pub(crate) async fn modhub_download<F>(
        &self,
        mod_id: u64,
        expected_file_name: &str,
        on_progress: F,
    ) -> Result<()>
    where
        F: Fn(crate::model::Progress) + Send + Sync,
    {
        self.modhub_start(mod_id).await?;
        let deadline = Instant::now() + MODHUB_DOWNLOAD_TIMEOUT;
        loop {
            let p = self.modhub_progress(mod_id).await?;
            on_progress(p);
            if let Some(total) = p.total {
                if total > 0 && p.done >= total {
                    break;
                }
            }
            if Instant::now() >= deadline {
                return Err(Error::NotProven);
            }
            tokio::time::sleep(MODHUB_POLL_INTERVAL).await;
        }

        let present = self
            .list_mods()
            .await?
            .iter()
            .any(|m| m.file_name == expected_file_name);
        if present {
            Ok(())
        } else {
            Err(Error::NotProven)
        }
    }

    /// Serverseitige ModHub-Kategorieseite lesen (`mods.html?category=<id>&page=<p>&lang=en`,
    /// Kap. 10.1 LH) — **nur bei gestopptem Server** (sonst zeigt `?category=` die installierten
    /// Mods statt des ModHub-Katalogs, Kap. 7.7 LH).
    pub(crate) async fn modhub_category(
        &self,
        category: u8,
        page: u32,
    ) -> Result<Vec<crate::model::ModhubCategoryEntry>> {
        if matches!(self.state().await?, ServerState::Online { .. }) {
            return Err(Error::ServerRunning);
        }
        let url = self
            .base_url
            .join(&format!("mods.html?category={category}&page={page}&lang=en"))
            .map_err(|e| Error::Parse(e.to_string()))?;
        let html = self.get_text(url).await?;
        crate::modhub::parse_category(&html)
    }

    /// Verfügbare Logs (Typen + Dateien) und die aktuelle `epoch` lesen (MZ5, Kap. 7.4 LH).
    pub(crate) async fn list_logs(&self) -> Result<crate::model::LogListing> {
        let url = self
            .base_url
            .join("logs.html?lang=en")
            .map_err(|e| Error::Parse(e.to_string()))?;
        crate::logs::parse_listing(&self.get_text(url).await?)
    }

    /// Log inkrementell lesen: alles ab `offset` der Datei `log_file` (Typ `log_type`), mit der
    /// `epoch` aus [`Self::list_logs`]. Rückgabe enthält den neuen Text, die nächste Byte-Position
    /// (`end_offset`) und ob das Log noch aktiv beschrieben wird (`tail -f`, Kap. 7.4 LH).
    pub(crate) async fn read_log(
        &self,
        log_type: u8,
        log_file: &str,
        offset: u64,
        epoch: u64,
    ) -> Result<crate::model::LogChunk> {
        let url = self
            .base_url
            .join(crate::logs::LONGPOLL_ENDPOINT)
            .map_err(|e| Error::Parse(e.to_string()))?;
        let body = vec![
            ("log_type".to_string(), log_type.to_string()),
            ("log_file".to_string(), log_file.to_string()),
            ("offset".to_string(), offset.to_string()),
            ("epoch".to_string(), epoch.to_string()),
        ];
        // Long-Poll: am Dateiende blockiert der Server bis neue Zeilen kommen. Per-Request-Timeout
        // setzen; läuft es ab **oder** kappt der Server die wartende Verbindung, gab es einfach
        // nichts Neues → leerer Abschnitt, `offset` unverändert (der Aufrufer pollt weiter). Das
        // ist beim Tailing normal und kein Fehler; eine echte Störung zeigt sich an
        // `state()`/`list_logs`, nicht am Log-Tail.
        let req = self.client.post(url).form(&body).timeout(LONGPOLL_TIMEOUT);
        // Senden UND Body-Lesen gemeinsam: der Server kann die wartende Verbindung auch erst
        // beim Lesen des Body kappen.
        let json: reqwest::Result<String> = async {
            let resp = self.send_raw(req).await?;
            resp.text().await
        }
        .await;
        match json {
            Ok(text) => crate::logs::parse_chunk(&text),
            Err(e) if is_longpoll_end(&e) => Ok(crate::model::LogChunk {
                content: String::new(),
                end_offset: offset,
                active: true,
            }),
            Err(e) => Err(map_reqwest(e)),
        }
    }

    /// URL der Mod-Verwaltungsseite (`mods.html?lang=en`) — Ziel für Löschen und Upload.
    fn mods_page_url(&self) -> Result<Url> {
        self.base_url
            .join("mods.html?lang=en")
            .map_err(|e| Error::Parse(e.to_string()))
    }

    /// Spiel-Einstellungen lesen (nur bei gestopptem Server, Kap. 6.1).
    pub(crate) async fn read_settings(&self) -> Result<crate::model::GameSettings> {
        crate::settings::parse_settings(&self.home().await?)
    }

    /// Verfügbare Dropdown-Optionen der Einstellungen lesen (G6; nur gestoppt).
    pub(crate) async fn read_settings_options(&self) -> Result<crate::model::SettingsOptions> {
        crate::settings::parse_settings_options(&self.home().await?)
    }

    /// Einstellungen als Textanzeige lesen (G6, nur bei **laufendem** Server — dort liefert das
    /// Panel kein Formular, siehe [`Self::read_settings`]).
    pub(crate) async fn read_settings_summary(&self) -> Result<Vec<crate::model::SettingsRow>> {
        crate::settings::parse_settings_readonly(&self.home().await?)
    }

    /// Spiel-Einstellungen speichern — **nur bei gestopptem Server** (Kap. 6). Voll-Formular-
    /// Umlauf: alle Felder aus `settings` senden + `save_settings`. Q4: fehlt das Formular/der
    /// Knopf → `FormMismatch`. Q3: danach zurücklesen und vergleichen — sonst `NotProven`.
    pub(crate) async fn save_settings(&self, settings: &crate::model::GameSettings) -> Result<()> {
        let home = self.home().await?;
        if matches!(parse_state(&home)?, ServerState::Online { .. }) {
            return Err(Error::ServerRunning);
        }
        let action = self.form_url(&home, "configuration")?;
        if !crate::settings::form_has_field(&home, "configuration", "save_settings") {
            return Err(Error::FormMismatch(
                "save_settings-Knopf fehlt (falscher Serverzustand?)".to_string(),
            ));
        }

        let savegame_empty = crate::settings::savegame_slot_empty(&home, settings.savegame);
        let mut body = crate::settings::settings_body(settings, savegame_empty);
        body.push(("save_settings".to_string(), "Save".to_string()));
        self.send(self.client.post(action).form(&body)).await?;

        // Q3: Ergebnis am zurückgelesenen Zustand belegen.
        let after = crate::settings::parse_settings(&self.home().await?)?;
        if crate::settings::settings_saved(settings, &after, savegame_empty) {
            Ok(())
        } else {
            Err(Error::NotProven)
        }
    }

    /// Mods aktivieren/deaktivieren — **nur bei gestopptem Server** (Kap. 7.3 LH).
    ///
    /// Deaktivieren läuft über das `ActiveMods`-Formular (`moddeactivate_<Datei>` +
    /// `deactivate_mods`), Aktivieren über `InactiveMods` (`modactivate_<Datei>` +
    /// `activate_mods`). Ergebnis wird belegt: jeder Mod muss danach im gegenteiligen Bereich
    /// stehen (Q3) — sonst `NotProven`. Fehlt ein erwartetes Formular → `FormMismatch` (Q4).
    pub(crate) async fn set_active(
        &self,
        activate: &[String],
        deactivate: &[String],
    ) -> Result<()> {
        if activate.is_empty() && deactivate.is_empty() {
            return Ok(());
        }

        // Guard: Umschalten geht nur offline (online fehlen die Mod-Checkboxen, Kap. 9.1).
        let home = self.home().await?;
        if matches!(parse_state(&home)?, ServerState::Online { .. }) {
            return Err(Error::ServerRunning);
        }

        if !deactivate.is_empty() {
            let action = self.form_url(&home, "ActiveMods")?;
            let body = crate::mods::toggle_body(
                deactivate,
                "moddeactivate_",
                "deactivate_mods",
                "Deactivate",
            );
            self.send(self.client.post(action).form(&body)).await?;
        }
        if !activate.is_empty() {
            let action = self.form_url(&home, "InactiveMods")?;
            let body =
                crate::mods::toggle_body(activate, "modactivate_", "activate_mods", "Activate");
            self.send(self.client.post(action).form(&body)).await?;
        }

        // Verifikation (Q3): jeder Mod muss jetzt im gegenteiligen Bereich stehen.
        let mods = crate::mods::parse_mods(&self.home().await?);
        let status_of = |file: &str| mods.iter().find(|m| m.file_name == file).map(|m| m.status);
        let ok = deactivate
            .iter()
            .all(|f| status_of(f) == Some(ModStatus::Inactive))
            && activate
                .iter()
                .all(|f| status_of(f) == Some(ModStatus::Active));
        if ok {
            Ok(())
        } else {
            Err(Error::NotProven)
        }
    }

    /// Absolute `action`-URL eines Formulars auf `html`; fehlt es, `FormMismatch` (Q4).
    fn form_url(&self, html: &str, form_name: &str) -> Result<Url> {
        form_action(html, form_name)
            .and_then(|a| self.base_url.join(&a).ok())
            .ok_or_else(|| Error::FormMismatch(format!("Formular {form_name} nicht gefunden")))
    }

    // --- Serversteuerung (F6) — Voll-Formular-Umlauf, Ergebnis am Zustand belegt ---

    /// Server starten (offline → online). Idempotent: läuft er schon, `Ok`.
    pub(crate) async fn start(&self) -> Result<()> {
        let home = self.home().await?;
        if matches!(parse_state(&home)?, ServerState::Online { .. }) {
            return Ok(());
        }
        self.submit_configuration(&home, "start_server", "Start")
            .await?;
        self.wait_until_online(true, START_TIMEOUT).await
    }

    /// Server stoppen (online → offline). Idempotent: ist er schon aus, `Ok`.
    pub(crate) async fn stop(&self) -> Result<()> {
        let home = self.home().await?;
        if matches!(parse_state(&home)?, ServerState::Offline) {
            return Ok(());
        }
        self.submit_configuration(&home, "stop_server", "Stop")
            .await?;
        self.wait_until_online(false, STOP_TIMEOUT).await
    }

    /// Server neu starten (online → offline → online). Nur bei laufendem Server (sonst fehlt
    /// `restart_server` im Formular → `FormMismatch`).
    pub(crate) async fn restart(&self) -> Result<()> {
        let home = self.home().await?;
        self.submit_configuration(&home, "restart_server", "Restart")
            .await?;
        // Übergang belegen: erst offline (fährt runter), dann wieder online.
        self.wait_until_online(false, STOP_TIMEOUT).await?;
        self.wait_until_online(true, START_TIMEOUT).await
    }

    /// `configuration`-Formular voll zurücksenden und den gewünschten Absende-Knopf ergänzen.
    /// Vor dem POST prüfen, dass der Knopf existiert (Q4) — sonst `FormMismatch`. Der Body (mit
    /// den Klartext-Passwörtern) wird **nie** protokolliert.
    async fn submit_configuration(&self, home: &str, button: &str, value: &str) -> Result<()> {
        let action = self.form_url(home, "configuration")?;
        if !crate::settings::form_has_field(home, "configuration", button) {
            return Err(Error::FormMismatch(format!(
                "{button} nicht im configuration-Formular (falscher Serverzustand?)"
            )));
        }
        let mut body = crate::settings::configuration_body(home)?;
        body.push((button.to_string(), value.to_string()));
        self.send(self.client.post(action).form(&body)).await?;
        Ok(())
    }

    /// Auf den gewünschten Laufzeitzustand warten (Ergebnis-Verifikation, Q3). Pollt `state`
    /// bis zum Ziel oder bis `timeout` — dann `NotProven` (abgesetzt, aber nicht bestätigt).
    async fn wait_until_online(&self, want_online: bool, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            let online = matches!(self.state().await?, ServerState::Online { .. });
            if online == want_online {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Error::NotProven);
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Authentifizierten GET der Home-Seite; erneuert die Sitzung **einmal transparent**, falls
    /// sie abgelaufen ist (Login-Formular zurück statt Home) — reaktiv, kein Pollen (Kap. 6).
    pub(crate) async fn home(&self) -> Result<String> {
        self.get_authenticated(self.base_url.clone()).await
    }

    /// Wie [`Self::home`], aber für eine beliebige Panel-Seite: authentifizierter GET, mit
    /// **einmal transparentem** Re-Login, falls die Sitzung abgelaufen ist (Login-Formular
    /// statt der angefragten Seite zurückkommt) — reaktiv, kein Pollen (Kap. 6).
    async fn get_authenticated(&self, url: Url) -> Result<String> {
        let body = self.get_text(url.clone()).await?;
        if !is_login_page(&body) {
            return Ok(body);
        }
        // Sitzung abgelaufen → einmal neu anmelden und die Seite erneut holen.
        self.authenticate().await?;
        let body = self.get_text(url).await?;
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
        self.send_raw(req).await.map_err(map_reqwest)
    }

    /// Wie [`Self::send`], aber mit dem **rohen** reqwest-Fehler — damit der Long-Poll ein
    /// Timeout von echter Unerreichbarkeit unterscheiden kann. Folgt Redirects (Post/
    /// Redirect/Get seit Patch 21) selbst mit dem aktuellen Cookie, da reqwests eigene
    /// Redirect-Behandlung deaktiviert ist (siehe Modulkopf).
    async fn send_raw(&self, req: RequestBuilder) -> reqwest::Result<Response> {
        let mut resp = self.send_once(req).await?;
        for _ in 0..MAX_REDIRECTS {
            let Some(location) = redirect_location(&resp) else {
                break;
            };
            resp = self.send_once(self.client.get(location)).await?;
        }
        Ok(resp)
    }

    /// Eine Anfrage mit aktueller SessionID abschicken (ohne Redirects zu folgen) und danach
    /// ein ggf. geändertes Cookie aus `Set-Cookie` übernehmen.
    async fn send_once(&self, req: RequestBuilder) -> reqwest::Result<Response> {
        let sid = self.session_id.lock().unwrap().clone();
        let req = if sid.is_empty() {
            req
        } else {
            req.header(COOKIE, format!("{SESSION_COOKIE}={sid}"))
        };
        let resp = req.send().await?;
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

/// Ziel-URL eines Redirects (3xx + `Location`), absolut aufgelöst gegen die angefragte URL —
/// `None`, wenn die Antwort keiner ist (Regelfall, da reqwests eigene Redirect-Behandlung
/// deaktiviert ist).
fn redirect_location(resp: &Response) -> Option<Url> {
    if !resp.status().is_redirection() {
        return None;
    }
    let location = resp.headers().get(reqwest::header::LOCATION)?.to_str().ok()?;
    resp.url().join(location).ok()
}

/// SessionID-Cookie aus einer Antwort lesen.
fn session_id(resp: &Response) -> Option<String> {
    resp.cookies()
        .find(|c| c.name() == SESSION_COOKIE)
        .map(|c| c.value().to_string())
}

/// `action`-Ziel eines benannten Formulars auslesen (z. B. `input`, `ActiveMods`,
/// `InactiveMods`) — meist `index.html?lang=…`. Der Server reagiert nur auf diese vollständige
/// URL (siehe Login).
fn form_action(html: &str, form_name: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let form = Selector::parse(&format!(r#"form[name="{form_name}"]"#)).unwrap();
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

/// Endete ein Log-Long-Poll ohne neue Daten? Wahr bei unserem Timeout **und** wenn der Server die
/// wartende Verbindung kappt (Verbindungs-/Request-/Body-Fehler). Beim Tailing normal — der
/// Aufrufer pollt dann einfach weiter.
fn is_longpoll_end(e: &reqwest::Error) -> bool {
    e.is_timeout() || e.is_connect() || e.is_request() || e.is_body()
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
            super::form_action(html, "input").as_deref(),
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
