# Pflichtenheft: Server Control for Farming Simulator 2025

**Version:** 0.1 (Entwurf)
**Datum:** 2026-07-23
**Bezug:** [Lastenheft-ServerControl.md](Lastenheft-ServerControl.md) — dieses Dokument
beantwortet das dort beschriebene *Was* mit dem technischen *Wie*.

> Lesehinweis: Kapitelverweise in Klammern (z. B. „MZ6", „Kap. 7.3", „G2") beziehen sich auf
> das ServerControl-Lastenheft, sofern nicht anders angegeben. Verweise auf „ModMatcher-PH"
> meinen [Pflichtenheft.md](Pflichtenheft.md).

---

## 1. Zweck und Abgrenzung

Das Lastenheft beschreibt die Anforderungen. Dieses Pflichtenheft legt fest, **mit welcher
Technik und in welcher Struktur** sie umgesetzt werden, und schließt die offen gelassenen
technischen Punkte (Lastenheft Kap. 8: Technologiewahl, Projektstruktur).

**Kernbegriff:** Server Control ist zuerst eine **Bibliothek** (der einzige Ort mit Logik);
**CLI und GUI** sind dünne, gleichberechtigte Schichten darauf (Lastenheft Kap. 4). Diese
Architektur ist aus eigenen Gründen sinnvoll (Testbarkeit, Trennung) und hält zugleich die Tür
offen, dass ModMatcher die Bibliothek *optional* später einbindet — entworfen wird diese
Kopplung jetzt aber **nicht** (Kap. 2.4).

---

## 2. Technologieentscheidung

### 2.1 Gewählter Stack: **Rust-Bibliothek + Tauri-GUI + CLI**

| Schicht | Technologie |
|---|---|
| Kernlogik (Sitzung, HTTP, Formulare, Mods, Steuerung, Log, ModHub) | **Rust-Crate** (`servercontrol-core`) |
| CLI | **Rust-Binary** (dünne Schicht auf der Crate) |
| GUI (Ansichten G1–G6) | **Tauri** (Rust-Kern + Web-Oberfläche) |
| Zugangsdaten | **OS-Credential-Store** über `keyring` |

### 2.2 Begründung (Bezug zu den Anforderungen)
Rust ist aus **eigenen Gründen** gewählt — **nicht**, weil ModMatcher es erzwänge (das
Verhältnis ist bewusst locker, siehe 2.4):
- **Riskante Server-Eingriffe sicher (Q2/Q3):** Das Werkzeug greift in fremde, ggf. bespielte
  Server ein (Stoppen, Mods umschalten, Upload). Rusts strenge Typ- und Fehlerprüfung ist hier
  ein echtes Sicherheitsnetz.
- **Q3 / Kap. 4 (Rückmeldungen im Sekundentakt):** Log-Mitlesen und Upload-/Download-Fortschritt
  brauchen einen typsicheren Direktpfad Bibliothek → GUI. Eine CLI-Zwischenschicht würde
  Fortschritt und Fehlerstruktur verlieren (Lastenheft Kap. 4) — daher ruft die GUI **nie** die
  CLI, sondern direkt die Crate.
- **Q1 / SZ2 (Zugangsdaten sicher):** `keyring` bindet plattformweit an den OS-Credential-Store;
  Passwörter verlassen den Store nur im Moment des POST und werden nie protokolliert.
- **Q4 / SZ1 (Versionstoleranz):** Rusts strenge Typ- und Fehlerprüfung erzwingt, dass
  unerwartete Formulare als Fehlerfall behandelt werden, statt blind zu posten.
- **Q5 (drei Plattformen):** Tauri und die reinen HTTP-/Krypto-Crates bauen für Windows, macOS
  und Linux aus einer Codebasis.
- **Arbeitsplatz bereits eingerichtet:** Die Entwicklungsumgebung ist für Rust/Tauri aufgesetzt
  (ModMatcher-PH Kap. 4); dieselben Bausteine (`reqwest`, `scraper`, `keyring`, `tokio`,
  `serde`) — keine zweite Toolchain, keine doppelte Lernkurve.
- **Optionale ModMatcher-Nähe (Zusatznutzen, kein Zwang):** Weil beide Rust sind, *könnte*
  ModMatcher Server Control später als Abhängigkeit nutzen. Das ist ein möglicher Bonus, nicht
  der Grund für die Wahl (2.4).

### 2.3 Verworfene Alternativen
- **Eigenständige Sprache (Go/Python/C#):** grundsätzlich möglich, brächte aber eine zweite
  Toolchain, schwächere Sicherheitsnetze bei den riskanten Server-Aktionen und verbaute die
  optionale ModMatcher-Nähe — ohne erkennbaren Vorteil.
- **GUI ruft die CLI:** im Lastenheft (Kap. 4) bereits als Fehlweg begründet (Verlust von
  Fortschritt/Fehlerstruktur).
- **Electron für die GUI:** unpassendes Verhältnis für ein schlankes Steuerwerkzeug (siehe
  ModMatcher-PH Kap. 2.3).

### 2.4 Verhältnis zu ModMatcher (eigenständig, Kopplung optional)
**Entscheidung:** Server Control steht **für sich**. Eine Einbindung in ModMatcher wird jetzt
**nicht** entworfen oder vorausgesetzt.

- Die beiden Werkzeuge dienen **verschiedenen Rollen:** ModMatcher ist für **Spieler/Clients**
  (lokalen Mod-Ordner an eine Soll-Liste angleichen), Server Control für **Admins** (fremden
  Server steuern). Die einzige Überschneidung ist, dass beide vom FS-Webserver *lesen* — und
  dafür hat ModMatcher bereits sein **eigenes** fs25-Modul (ModMatcher-PH Kap. 7). Zum Lesen
  braucht es Server Control also nicht.
- **Offen gehalten, nicht gebaut:** Sollte ModMatcher je selbst **eingreifende** Server-Aktionen
  anbieten wollen (Mods am Server umschalten, starten/stoppen), *kann* es Server Control dann
  als **Abhängigkeit** einbinden, statt sie nachzubauen. Die Bibliothek-zuerst-Architektur hält
  diese Tür offen, ohne dass wir die Kopplung heute spezifizieren.
- Die **Bibliothek-zuerst-Architektur** (Kern + dünne CLI/GUI) bleibt davon unberührt — sie ist
  aus eigenen Gründen sinnvoll (Testbarkeit, saubere Trennung, Lastenheft Kap. 4).

---

## 3. Projektstruktur

**Eigenes Repository** (Entscheidung), getrennt von ModMatcher — eigener Release-Zyklus,
eigenständig nutzbar, sauberer Schnitt. Falls ModMatcher es je einbindet, dann *optional* als
**Abhängigkeit** (Crate über Git/Registry), nicht als Unterordner (Kap. 2.4). Diese
Konzeptdokumente (Lasten-/Pflichtenheft) bleiben hier; der Code entsteht im neuen Repo
`servercontrol`.

```
servercontrol/
├─ crates/
│  └─ core/            servercontrol-core: die Bibliothek (der einzige Ort mit Logik)
│     ├─ session       Anmeldung, Cookie-Sitzung, Abmelden (Kap. 7.3 LH)
│     ├─ http          HTTP-Client, Formular-POST, Versionstoleranz-Prüfung
│     ├─ mods          Mod-Liste lesen, aktivieren/deaktivieren, Upload, Löschen
│     ├─ control       start/stop/restart (Voll-Formular-Umlauf, Kap. 7.3 LH)
│     ├─ settings      Spiel-Einstellungen lesen/schreiben
│     ├─ logs          Live-Log (offset/epoch-Polling, Kap. 7.4 LH)
│     ├─ modhub        Server-Download (startmoddownload/Progress) + Website-Suche (Weg B)
│     └─ verify        Ergebnisnachweis: Zustand primär, Log für Gründe (Q3, Kap. 9)
├─ cli/                servercontrol-cli: dünnes Binary auf der Crate
├─ src-tauri/          Tauri-Anwendung: verbindet Crate und Oberfläche
├─ src/                Web-Oberfläche (G1–G6, Theme, Sprachdateien)
├─ tools/              Prüf-/Aufnahmeskripte (z. B. Formular-Abgleich gegen echten Server)
└─ .github/workflows/  CI: Build-Matrix Windows/macOS/Linux
```

Die **Crate enthält die gesamte Logik**; `cli/` und `src-tauri/` sind austauschbare Schichten.

---

## 4. Die Bibliotheks-Schnittstelle (Kern)

Umsetzung von MZ6 / Kap. 4. Alles, was dauern kann, ist **asynchron** und bekommt einen
`OpCtx` (Abbruch + Fortschritt) — analog zur ModMatcher-Trait-Schnittstelle.

### 4.1 Leitprinzipien
1. **Die Bibliothek sieht Zugangsdaten nur flüchtig.** Ein Profil trägt einen **Schlüssel** in
   den Credential-Store, nie das Passwort selbst. Beim POST löst die Bibliothek es intern auf
   und hält es nie in Log, Fehlermeldung oder temporärer Datei (Q1).
2. **Vor jedem Schreibvorgang: Formular gegenprüfen.** Erwartete Feldnamen müssen vorhanden
   sein; fehlen sie, bricht die Operation mit `FormMismatch` ab, statt zu posten (Q4/SZ1).
3. **Kein Erfolg ohne Beleg.** Eingreifende Aktionen liefern erst dann „fertig", wenn der
   **beobachtbare Zustand** es bestätigt (Server online/offline, Mod im gegenteiligen Formular,
   Datei in der Liste); das Log dient nur den **Fehlergründen**. Bleibt der Nachweis aus, meldet
   die Bibliothek `NotProven` statt „fertig" (Q3, ausgearbeitet in Kap. 9 — siehe `verify`).
4. **Start ist heikel, Aktivieren ist harmlos.** `start`/`save_settings` (offline) führen den
   **vollen Formular-Umlauf** aus (alle Felder unverändert mitsenden, Kap. 7.3). `stop`/`restart`
   (online) tragen **keine** Einstellungsfelder (verifiziert 0) → kein Umlauf nötig. Die
   Mod-Formulare enthalten nur Checkboxen.

### 4.2 Zustände und Kerntypen (Entwurf)

```rust
/// Verbindungsprofil. Enthält KEIN Passwort — nur den Verweis in den Credential-Store.
pub struct ServerProfile {
    pub name: String,
    pub base_url: Url,          // http ODER https (Kap. 8 LH: Schema folgenlos)
    pub username: String,
    pub credential_key: String, // Schlüssel in den OS-Credential-Store
    pub accept_invalid_cert: bool, // Randfall gemietete Server (Kap. 8 LH)
    pub file_access: Option<FileAccess>, // FTP/SFTP-Zugang (MZ4); getrennt vom Web-Login
}

/// FTP/SFTP-Zugang zum Serverordner (MZ4). Oft anderer Host/Port als das Web-Panel und mit
/// eigenen Zugangsdaten — daher ein **eigener** Credential-Store-Verweis, nie ein Passwort.
pub struct FileAccess {
    pub protocol: FileProtocol,   // Ftp | Sftp
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credential_key: String,   // eigener Eintrag im Credential-Store
    pub mods_path: String,        // Pfad zum mods/-Ordner auf dem Server
}
pub enum FileProtocol { Ftp, Sftp }

/// Laufzeitzustand des Spielservers. Erkennung über `div.status-indicator` (Kap. 9.1).
/// `version` = **Spielversion** aus dem Statistik-Block (z. B. "1.19.0.0"), nur online verfügbar
/// (daher `Option`) — NICHT der Web-Interface-Build aus dem Footer (z. B. "10.0.0.0").
pub enum ServerState { Online { version: Option<String> }, Offline }

/// Status eines Mods aus Sicht des Servers (Kap. 7.3, 10.5 LH).
pub enum ModStatus { Active, Inactive, Orphan } // Orphan = Registry-Eintrag ohne Datei

pub struct ServerMod {
    pub file_name: String,      // Kennung für modactivate_/moddeactivate_ (Kap. 7.3 LH)
    pub display_name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub size: Option<u64>,
    pub is_dlc: bool,
    pub status: ModStatus,
}

/// Ein Savegame-Slot aus Sicht des Servers (`savegames.html`, Kap. 7.8 LH). Nur belegte Slots
/// erscheinen in `list_savegames`; leere Slots sind reine Ziel-Optionen beim Upload
/// (`list_savegame_upload_slots`, s. u.).
pub struct ServerSavegame {
    pub slot: u8,                // 1..=20, Kennung für Download-Link `savegame<slot>` und delete_<slot>
    pub display_name: String,    // "My game save (1)"
    pub map: String,
    pub money: u64,
    pub play_time_minutes: u32,
    pub difficulty: Difficulty,
    /// Trägt die Zeile einen Lösch-Link? Live verifiziert: fehlt **nur** beim aktuell geladenen
    /// Savegame — ein normaler Slot hat ihn auch bei laufendem Server (Kap. 7.8 LH).
    pub can_delete: bool,
}

/// Automatisch vom Server angelegtes Zeitstempel-Backup eines Slots (Kap. 7.8 LH), Kennung
/// im Format `<slot>_<YYYY-MM-DD>_<HH-MM>` — genau der Wert, der als `backup_restore` gesendet
/// wird. Wiederherstellen überschreibt den Slot vollständig.
pub struct SavegameBackup {
    pub slot: u8,
    pub timestamp: String,       // z. B. "2026-07-13_23-56", unverändert als Formularwert genutzt
    pub map: String,
    pub play_time_minutes: u32,
}

/// Spiel-Einstellungen = Felder des `configuration`-Formulars (Kap. 6.1), Wertebereiche am
/// Server verifiziert. Genutzt von `read_settings`/`save_settings` (G6, Kann).
pub struct GameSettings {
    pub game_name: String,
    pub admin_password: Secret,        // am Server Klartextfeld; hier `Secret` → nie geloggt (Q1)
    pub game_password: Secret,
    pub savegame: u8,                  // Slot 1..=20
    // --- nur bei LEEREM Savegame editierbar (checkSavegame, Kap. 6) ---
    pub map_start: String,             // "default_MapEU" … oder "<Mod>.zip_<MapId>" (auch Mod-Maps)
    pub initial_money: u32,            // 100000|250000|500000|750000|1000000
    pub initial_loan: u32,             // 0|100000|250000|500000|750000|1000000
    pub economic_difficulty: Difficulty,
    // --- immer editierbar ---
    pub server_port: u16,
    pub max_player: u8,                // 2..=16
    pub mp_language: String,           // Sprachcode "en"/"de"/… (27 Werte)
    pub auto_save_interval: u32,       // Minuten
    pub stats_interval: u32,           // Sekunden ("Web API Interval", Kap. 7.5 LH; groß = Feed aus)
    pub pause_game_if_empty: PauseIfEmpty,
    pub crossplay_allowed: bool,
}
pub enum Difficulty { Easy = 1, Normal = 2, Hard = 3 }        // Server-Werte
pub enum PauseIfEmpty { No = 1, Instantly = 2 }

/// Abbruch + Fortschritt für lange Operationen (Upload, Download, Warten auf Logmarke).
pub struct OpCtx { pub cancel: CancellationToken, pub progress: Arc<dyn ProgressSink> }
```

**`Secret` erzwingt Q1 auf Typ-Ebene.** Passwörter (`admin_password`, `game_password` und der
Credential-Store-Wert) sind vom Typ **`Secret`** — ein Wrapper, der (a) seinen Inhalt **nicht
ausdruckt** (in Log/Debug/Fehlermeldung erscheint nur `***`) und (b) den **Speicher beim
Freigeben nullt**. Damit lässt sich „nie Passwörter protokollieren" gar nicht erst versehentlich
verletzen. Kandidaten: `secrecy` (`SecretString`) bzw. `zeroize`.

> Die übrigen Hilfstypen (`LogFile`, `LogRef`, `LogMarker`, `LogHit`, `Progress`, `RemoteEntry`,
> `CatalogDetails`, `ProgressSink`) sind **Platzhalter des Entwurfs** und werden bei der
> Umsetzung ausdefiniert; `Result`/`Url`/`Path`/`Stream` sind Standard-/Bibliothekstypen.

### 4.3 Die Operationen (Entwurf)

> **Vertrag der eingreifenden Operationen:** `start`/`stop`/`restart` sowie `set_active`,
> `upload_mod`, `delete_mod`, `modhub_download`, `upload_savegame`, `delete_savegame` und
> `restore_savegame_backup` **verifizieren selbst** (Kap. 9). Sie geben
> `Ok` erst bei **nachgewiesenem** Ergebnis zurück, bei Zeitüberschreitung `NotProven`, bei
> Abbruch `Cancelled`; Fortschritt/Warten melden sie über `ctx`. Der Aufrufer muss nicht
> selbst nachprüfen. `await_log` ist dabei nur der **interne Baustein** für den ergänzenden
> Log-Blick (Fehlergründe), kein Pflichtaufruf.

```rust
impl ServerControl {
    /// Anmelden, Cookie-Sitzung aufbauen, Version erkennen (F2, Kap. 7.3 LH).
    pub async fn connect(profile: &ServerProfile, ctx: &OpCtx) -> Result<Self>;
    pub async fn state(&self) -> Result<ServerState>;
    pub async fn logout(self) -> Result<()>;               // index.html?logout=true

    // --- Mods (F3/F4) ---
    pub async fn list_mods(&self) -> Result<Vec<ServerMod>>;
    /// Nur bei GESTOPPTEM Server möglich (Kap. 7.3 LH: sonst FormMismatch/ServerRunning).
    pub async fn set_active(&self, activate: &[String], deactivate: &[String]) -> Result<()>;
    /// Wählt den Weg automatisch: Web-Formular (`modUpload`) bis 1,71 GB, **darüber FTP/SFTP**
    /// (MZ4). Ist die Datei zu groß und kein `file_access` konfiguriert → `NoFileAccess`.
    /// Upload geht in **beiden** Zuständen (nicht auf gestoppt beschränkt, verifiziert) —
    /// das laufende Spiel nutzt die Datei erst nach Neustart.
    pub async fn upload_mod(&self, path: &Path, ctx: &OpCtx) -> Result<()>;
    /// Ermittelt vorher über `list_mods` den Status und wählt das passende Feld:
    /// `deleteactive_<Datei>` (aktiv) bzw. `deleteinactive_<Datei>` (inaktiv). Nur bei
    /// GESTOPPTEM Server (online 0 Lösch-Buttons, verifiziert) → sonst `ServerRunning`.
    pub async fn delete_mod(&self, file_name: &str) -> Result<()>;

    // --- Savegames (Kann, Kap. 1.3 LH / Kap. 7.8 LH) — natives Web-Formular, wie bei Mods ---
    pub async fn list_savegames(&self) -> Result<Vec<ServerSavegame>>;
    /// Die echten `index_upload`-Dropdown-Optionen (belegte Slots + „SAVEGAME n" für leere) —
    /// **nicht** aus `list_savegames` synthetisiert: Das aktuell geladene Savegame fehlt darin
    /// live verifiziert, lässt sich aber nicht aus der Slot-Liste allein ableiten (die zeigt nur
    /// `can_delete = false`, nicht „fehlt im Upload-Dropdown").
    pub async fn list_savegame_upload_slots(&self) -> Result<Vec<FieldOption>>;
    /// GET `savegame<slot>` — einfacher Datei-Download, kein Formular-Umlauf nötig.
    pub async fn download_savegame(&self, slot: u8, local: &Path, ctx: &OpCtx) -> Result<()>;
    /// Wählt wie `upload_mod` automatisch den Weg: Web-Formular (`index_upload`+`file`) bis
    /// 1,71 GB, **darüber FTP/SFTP** (MZ4) sofern `file_access` konfiguriert, sonst
    /// `NoFileAccess`. `slot` = Zielslot (belegt oder leer, aus `list_savegame_upload_slots`),
    /// `name` = optionaler `custom_name`. Geht in **beiden** Zuständen (verifiziert, wie
    /// `upload_mod` — **nicht** wie `set_active`).
    pub async fn upload_savegame(&self, slot: u8, name: Option<&str>, path: &Path, ctx: &OpCtx) -> Result<()>;
    /// `savegames.html?delete_<slot>=true`. Geht in **beiden** Zuständen (verifiziert) — **außer**
    /// beim aktuell geladenen Savegame (`ServerSavegame.can_delete = false`), das lässt sich
    /// grundsätzlich nicht löschen, unabhängig vom Serverzustand.
    pub async fn delete_savegame(&self, slot: u8, ctx: &OpCtx) -> Result<()>;
    pub async fn list_savegame_backups(&self, slot: u8) -> Result<Vec<SavegameBackup>>;
    /// `backup_restore=<slot>_<timestamp>` — überschreibt den Slot, daher bestätigungspflichtig
    /// in der GUI (Q2, G7). Geht in **beiden** Zuständen (verifiziert).
    pub async fn restore_savegame_backup(&self, backup: &SavegameBackup, ctx: &OpCtx) -> Result<()>;

    // --- Allgemeiner Dateizugriff über FTP/SFTP (MZ4; Kann, Lastenheft 1.3) ---
    pub async fn put_file(&self, local: &Path, remote: &str, ctx: &OpCtx) -> Result<()>;
    pub async fn get_file(&self, remote: &str, local: &Path, ctx: &OpCtx) -> Result<()>;
    pub async fn list_dir(&self, remote: &str) -> Result<Vec<RemoteEntry>>;

    // --- Steuerung (F6) — Voll-Formular-Umlauf ---
    pub async fn start(&self, ctx: &OpCtx) -> Result<()>;   // configuration + start_server
    pub async fn stop(&self, ctx: &OpCtx) -> Result<()>;    // stop_server
    pub async fn restart(&self, ctx: &OpCtx) -> Result<()>; // restart_server

    // --- Einstellungen (Kann/G6) ---
    pub async fn read_settings(&self) -> Result<GameSettings>;
    pub async fn save_settings(&self, s: &GameSettings, ctx: &OpCtx) -> Result<()>; // nur gestoppt

    // --- Log (F7/Q3) ---
    pub async fn list_logs(&self) -> Result<Vec<LogFile>>;
    /// Fortlaufender Strom neuer Zeilen (offset/epoch-Polling, Kap. 7.4 LH).
    pub fn tail(&self, log: &LogRef) -> impl Stream<Item = Result<String>>;
    /// Wartet, bis eine Marke im Log erscheint (z. B. "Game server started") — Q3.
    pub async fn await_log(&self, marker: &LogMarker, ctx: &OpCtx) -> Result<LogHit>;

    // --- ModHub: Download durch den Server (F5, Kap. 7.7 LH) ---
    // NUR bei GESTOPPTEM Server: bei laufendem Server bietet der ModHub keine Downloads an
    // (verifiziert: 0 startmoddownload-Buttons, Kap. 7.7 LH) → sonst `ServerRunning`.
    pub async fn modhub_start(&self, mod_id: u64) -> Result<()>;        // startmoddownload=<id>
    pub async fn modhub_progress(&self, mod_id: u64) -> Result<Progress>; // JSON {downloaded,total}
    pub async fn modhub_cancel(&self, mod_id: u64) -> Result<()>;       // cancelmoddownload
    /// Bequemlichkeit: startet und pollt bis fertig, meldet Fortschritt über ctx.
    pub async fn modhub_download(&self, mod_id: u64, ctx: &OpCtx) -> Result<()>;
}
```

### 4.4 ModHub-Namenssuche (Weg B, Kann-Ziel 1.3 / Kap. 7.7 LH)

Eigenes Modul `modhub::catalog` gegen die **öffentliche ModHub-Website** — getrennt von der
Server-Schnittstelle, weil es einen anderen Host anspricht.

```rust
pub mod catalog {
    /// GET farming-simulator.com/mods.php?title=fs2025&searchMod=<query>
    pub async fn search(query: &str, ctx: &OpCtx) -> Result<Vec<CatalogEntry>>;
    /// GET mod.php?mod_id=<id> — Detailinfos für die Trefferansicht.
    pub async fn details(mod_id: u64, ctx: &OpCtx) -> Result<CatalogDetails>;
}
pub struct CatalogEntry { pub mod_id: u64, pub name: String, pub author: Option<String>,
                          pub rating: Option<f32>, pub thumb_url: Option<Url> }
```

Die `mod_id` ist **identisch mit der `startmoddownload`-ID** des Servers (an `366506`
verifiziert, Kap. 7.7 LH) — „Install on server" reicht sie direkt an `modhub_download`.

> **Versionstoleranz auch hier (SZ1/Q4):** Die Website liefert **kein API**, nur gerendertes
> HTML. Der Parser prüft erwartete Strukturen und meldet `ParseError`, statt Müll zu liefern,
> falls GIANTS das Layout ändert.

### 4.5 Fehlerbehandlung

| Fehlerfall | Beispiel | Reaktion der Oberfläche |
|---|---|---|
| `Unreachable` | Panel offline, Host weg | „Server nicht erreichbar" |
| `AuthFailed` | Login abgelehnt (Daten falsch) | Zugangsdaten prüfen |
| `CredentialMissing` | Profil da, aber kein Passwort im Credential-Store (extern gelöscht / Profil auf anderen Rechner kopiert) | gezielt Passwort neu eingeben (≠ `AuthFailed`) |
| `FormMismatch` | erwartetes Feld fehlt | „Serverformat nicht erkannt" — nicht posten (Q4) |
| `ServerRunning` | Mod-Umschaltung bei laufendem Server (Kap. 7.3 LH) | „zum Ändern erst stoppen" |
| `NoFileAccess` | Datei > 1,71 GB, aber **kein FTP/SFTP** im Profil hinterlegt (Kap. 10.10 LH) | „Für große Dateien FTP/SFTP im Profil einrichten" |
| `FileAuthFailed` | FTP/SFTP-Login abgelehnt (eigener Zugang, ≠ Web-Login) | „FTP/SFTP-Zugangsdaten prüfen" |
| `FileUnreachable` | FTP/SFTP-Host/Port nicht erreichbar | „FTP/SFTP-Server/Port prüfen" |
| `TransferFailed` | Abbruch im Up-/Download (HTTP **oder** FTP/SFTP) | Wiederholen |
| `NotProven` | Aktion abgesetzt, aber keine Logmarke (Q3) | ehrliche Warnung statt „fertig" |
| `Cancelled` | Nutzer-Abbruch | kein Fehler — sauber zurücksetzen |

---

## 5. Vorgesehene Bibliotheken (Kandidaten)

| Aufgabe | Anforderung | Kandidat |
|---|---|---|
| HTTP inkl. Cookie-Sitzung, Uploads, HTTPS | F2–F7, Kap. 7.3 | `reqwest` (+ `cookie_store`) |
| HTML der Formulare/Mods/ModHub auswerten | Kap. 7.3, 7.7 | `scraper` |
| JSON des Fortschritts-Endpunkts | Kap. 7.7 | `serde_json` |
| OS-Credential-Store | Q1/SZ2 | `keyring` |
| Passwörter im Speicher schützen (`Secret`) | Q1/SZ2 | `secrecy` / `zeroize` |
| Nebenläufigkeit, Ströme (Log-Tail, Progress) | F7, Q3 | `tokio` + `tokio-stream` |
| Konfiguration/Profile serialisieren | F1 | `serde` / `serde_json` |
| URL-Bau (Schema http/https, Query) | F1, Kap. 8 | `url` |
| **FTP** (große Uploads >1,71 GB, Dateizugriff) | **MZ4**, F5 Weg 3 | `suppaftp` |
| **SFTP** (dito über SSH) | **MZ4**, F5 Weg 3 | `russh-sftp` bzw. `ssh2` |

FTP/SFTP ist **fester Bestandteil** (MZ4), kein Kann — nötig für Uploads jenseits der
1,71-GB-Web-Grenze und den optionalen Dateizugriff (Savegames, Mod-Sets). Bibliotheken analog
ModMatcher-PH Kap. 5.

---

## 6. Sitzung, Formulare und Versionstoleranz

- **Anmeldung** (Kap. 7.3 LH): GET der Login-Seite (setzt die erste `SessionID`, liefert die
  Formular-`action` mit `?lang=…`) → POST `index.html?lang=…` mit `username`+`password`+
  `login=Login`; Sitzung über `Set-Cookie`; **kein CSRF-Token, keine versteckten Felder** →
  kein Sonderaufwand. Abmelden über `index.html?logout=true`. **Erfolg nicht an der POST-Antwort
  ablesen** — die rendert noch die Login-Seite (JS/Meta-Reload, dem ein HTTP-Client nicht folgt);
  stattdessen an einem **frischen GET** danach prüfen (Login-Formular weg = angemeldet).
  HTTP-Eigenheiten des Panels dabei zwingend beachten → **6.2**.
- **Formular-Umlauf beim Start** (Kap. 7.3 LH): `start`/`save_settings` lesen zuerst das
  vollständige `configuration`-Formular, senden **alle** Felder unverändert zurück und ergänzen
  nur den Absende-Knopf. `admin_password`/`game_password` laufen dabei als Klartextfelder durch
  und werden **nie protokolliert**. Betrifft **nur `start`/`save_settings`** (offline, wo die
  Einstellungsfelder editierbar sind): `stop`/`restart` laufen online, wo das `configuration`-
  Formular **nur die zwei Knöpfe und keine Einstellungsfelder** trägt (verifiziert: 0 Felder) —
  dort gibt es also nichts zu überschreiben, kein Umlauf nötig.
  - **Deaktivierte Felder spiegeln (verifiziert):** Bei einem **bestehenden** Savegame sind
    `map_start`, `initialMoney`, `initialLoan`, `economicDifficulty` **`disabled`** (JS
    `checkSavegame()`: `disabled = !isEmptySavegame`); nur bei einem **leeren** Slot (= neues
    Spiel) sind sie editierbar. Der Browser sendet `disabled`-Felder **nicht** mit — die
    Bibliothek muss das **nachbilden** und diese vier Felder bei bestehendem Savegame
    **weglassen**, sonst weicht sie vom Browserverhalten ab.
- **Versionstoleranz** (Q4/SZ1): Vor jedem Schreib-POST prüft die Bibliothek die erwarteten
  Feldnamen (Tabelle je Formular). Abweichung → `FormMismatch`, klare Meldung, kein Post.
- **Öffentliches-Panel-Warnung** (Kap. 7.3 LH): Erkennt die Bibliothek ein ohne VPN/IP-Grenze
  erreichbares Panel, meldet sie das der Oberfläche (Sicherheitshinweis für G1).
- **Sitzungsablauf — reaktiv, kein Pollen:** Cookie-Sitzungen laufen ab (Zeit, Server-Neustart).
  Die Bibliothek prüft das **nicht** im Hintergrund, sondern **bei jeder Anfrage**: Kommt statt
  der erwarteten Seite das **Login-Formular** zurück (`name="input"` + Passwortfeld), gilt die
  Sitzung als abgelaufen. Sie meldet sich dann **einmal transparent neu an** (Zugang aus dem
  Credential-Store) und **wiederholt** die Anfrage — der Nutzer merkt idealerweise nichts.
  Scheitert das Re-Login, `AuthFailed` (sichtbar). Passt zum „kein Hintergrund-Check" aus 7.4.

### 6.1 Erwartete Felder je Formular (Versionstoleranz-Referenz)

Der konkrete Prüfmaßstab für Prinzip 2 (Kap. 4.1) und den Formular-Abgleich-Test (Kap. 11.1) —
**am lebenden Server verifiziert**:

| Formular | `action` | Methode | Pflichtfelder | Absende-Knopf |
|---|---|---|---|---|
| Login (`input`) | `index.html?lang=…` | POST | `username`, `password` | `login` |
| `configuration` | `index.html?lang=…` | POST | `game_name`, `admin_password`, `game_password`, `savegame`, `map_start`, `initialMoney`, `initialLoan`, `economicDifficulty`, `server_port`, `max_player`, `mp_language`, `auto_save_interval`, `stats_interval`, `pause_game_if_empty`, `crossplay_allowed` | offline: `save_settings` + `start_server` · online: `stop_server` + `restart_server` |
| `ActiveMods` | `index.html?lang=…#mods` | POST | `moddeactivate_<Datei>` *(nur gestoppt)* | `deactivate_mods` |
| `InactiveMods` | `index.html?lang=…#mods` | POST | `modactivate_<Datei>` *(nur gestoppt)* | `activate_mods` |
| Löschen (unbenannt) | `mods.html?lang=…` | POST | `deleteactive_<Datei>` / `deleteinactive_<Datei>` | je Mod ein Knopf |
| `modUpload` | `mods.html?lang=…#modUpload` | POST | `file` | `file_upload` |
| ModHub-Download | `mods.html?category=…` | POST | `startmoddownload=<id>`; Abbruch `cancelmoddownload`; Fortschritt `modhubdownloadprogress` (GET) | Button = Feld |

> **Zustandsabhängigkeit beachten:** Bei laufendem Server fehlen `save_settings`/`start_server`
> (stattdessen `stop_server`/`restart_server`) und die Mod-Checkboxen (Kap. 9.1). Das ist
> **erwartet** — die Prüfung darf es nicht als `FormMismatch` werten. Der Abgleich erfolgt daher
> **je Zustand** gegen die passende Spalte.

### 6.2 HTTP-Eigenheiten des FS-Panels (am lebenden Server verifiziert)

Das Panel (GIANTS-eigener HTTP-Server) weicht in einem Punkt hart von üblichem HTTP-Verhalten
ab. Der kostete in der Umsetzung viel Fehlersuche — daher hier festgehalten:

- **`Cookie`-Header case-sensitiv (die eigentliche Falle).** Der Server erkennt den
  Sitzungs-Cookie **nur** bei exakt `Cookie:` (großes C). Kleingeschriebene Header-Namen
  (`cookie:`) — wie sie hyper/reqwest per Default schreibt — werden **ignoriert**; jede Anfrage
  wirkt dann wie eine neue, nicht angemeldete Sitzung, und der Login greift nie. Umsetzung in
  Rust: `reqwest::ClientBuilder::http1_title_case_headers()` (schreibt `Cookie`, `Content-Type`
  … in Title-Case). *Verifiziert:* mit `Cookie:` behält der Server die `SessionID` bei, mit
  `cookie:` vergibt er bei jeder Antwort eine neue.
- **Cookie selbst führen, nicht pinnen.** Der Server schickt in jeder Antwort ein `Set-Cookie`.
  Solange der Cookie (dank Title-Case) erkannt wird, bleibt die `SessionID` **stabil**
  (verifiziert). Ob der Server sie beim Login wechselt (Session-Fixation-Schutz), ließ sich
  nicht sauber isolieren — die Bibliothek **folgt** dem Cookie deshalb nach jeder Antwort
  (übernimmt die ID aus `Set-Cookie`) statt eine feste ID zu pinnen; das ist in beiden Fällen
  robust. reqwests eingebauter Cookie-Speicher wird **nicht** genutzt.
- **Kein CSRF, kein Passwort-Hashing, keine IP-Bindung** (alle verifiziert): Der Login ist ein
  reiner Klartext-Formular-POST; das JS auf der Seite hasht nichts. Auth hängt am Cookie, nicht
  an der Client-IP (ein GET ohne gültigen Cookie ist stets nicht angemeldet). Ein `User-Agent`
  ist für die Sitzung **nicht** erforderlich (verifiziert) — die Bibliothek setzt trotzdem einen
  zur sauberen Client-Kennzeichnung.

---

## 7. GUI-Spezifikation (G1–G7)

Web-Oberfläche unter Tauri. Dieser Abschnitt legt Aufbau, Zustände, Sperr-Logik,
Bestätigungen und das Event-Schema fest.

### 7.0 Interaktiver Mockup

Zu diesem Kapitel gehört ein **interaktiver HTML-Mockup** als visuelle Referenz:
[`mockups/servercontrol-mockup.html`](mockups/servercontrol-mockup.html) (im Repo; zusätzlich
als privates Artifact veröffentlicht).

- **Umfang:** der **App-Rahmen** (Statusleiste + Seitenleiste) und die zentrale Ansicht
  **G2 (Mod-Übersicht)**; dazu das **Server-Dropdown** mit Wechseln, **„Verbindung trennen"**
  (GUI-Seite von `logout`) und „Server verwalten…".
- **Was er zeigt:** die drei Zustände **nicht verbunden / verbunden+gestoppt / verbunden+läuft**
  und die daran hängende **Sperr-Logik** (7.2) — beim laufenden Server werden Aktivierung,
  Löschen und ModHub-Install gesperrt, mit Banner „zum Ändern erst stoppen". Weiter: die
  Status-Badges **Aktiv / Inaktiv / Karteileiche**, ein Mod mit Fehlerhinweis, sowie
  **Sortierung per Klick auf den Spaltenkopf** (Auf-/Absteigend-Pfeil) statt eines Dropdowns.
- **Charakter:** **Design-Vorlage mit Beispieldaten**, keine Logik und keine Live-Verbindung.
  Da die echte Oberfläche ohnehin HTML/CSS unter Tauri ist (Kap. 2/3), kann der Mockup das
  spätere `src/`-Frontend **anschieben** (Layout, Komponenten, Theme) — er ist kein
  Wegwerf-Artefakt.
- **Gestaltung:** theme-fähig (hell/dunkel folgen dem System), **Ocker-Akzent** (Ernte/Korn),
  semantische Farben **grün/grau/rot** strikt getrennt vom Akzent; Dateinamen/Versionen/Größen
  in Monospace (Server-/Log-Anmutung).
- **Icon-Set: [Tabler Icons](https://tabler.io/icons)** (Entscheidung) — MIT-Lizenz
  (© 2020–2026 Paweł Kuna). Lizenz/Attribution liegt bei unter
  [`mockups/ICON-CREDITS.md`](mockups/ICON-CREDITS.md); die MIT-Pflicht (Copyright-/Lizenzhinweis
  mitliefern) ist damit erfüllt.

Der Mockup ist der **visuelle Stand**, die folgenden Abschnitte 7.1–7.11 die **verbindliche
Spezifikation**; bei Abweichungen gilt der Text.

### 7.1 Aufbau und Navigation

Eine **Seitenleiste** mit den Ansichten, oben eine **globale Statusleiste**:

```
┌────────────┬─────────────────────────────────────────────┐
│            │ [Server: ccc222 ▾] ● Online 1.19  [Stoppen] ⟳ │  Statusleiste (G1 + Steuerung)
│  Spielein- ├─────────────────────────────────────────────┤
│   stell.   │                                             │
│  Mods      │            aktive Ansicht                   │
│  Bereit-   │                                             │
│   stellen  │                                             │
│  Savegames │                                             │
│  Log       │                                             │
└────────────┴─────────────────────────────────────────────┘
```

**Getrennte Ansichten** (Grundsatz 4.1 LH): Spieleinstellungen, Mod-Übersicht, Bereitstellen,
Savegames und Log sind **eigene** Seitenleisten-Ansichten — bewusst **nicht** die
Ein-Seiten-Struktur des Herstellers (die Savegames selbst zwar schon als eigene Seite
`savegames.html` führt, aber die drei Teilaufgaben darauf untereinander stapelt statt in Reiter
zu trennen, Kap. 7.8 LH). Die **Serversteuerung** (Starten/Stoppen/Neustart) sitzt dagegen in der
**Statusleiste** neben dem Zustand (7.7), nicht als eigener Menüpunkt. Die Statusleiste
(Serverwahl + Zustand + Steuerung) ist immer sichtbar (G1).

**Zwei Ebenen, klar getrennt:** Die Seitenleiste sind **Aktionen am aktiven Server**. *Welcher*
Server aktiv ist, wählt man eine Ebene darüber — in der Statusleiste, nicht in der Seitenleiste.
Das Server-Dropdown listet die Profile zum **schnellen Wechseln** (Live-Status nur beim
**aktiven** Profil — kein Hintergrund-Check, Kap. 7.4) und trägt unten **„Verbindung trennen"**
(GUI-Seite von `logout`, führt in den Zustand *nicht verbunden*) und **„Server verwalten…"**
(öffnet den Profil-Verwaltungsbildschirm, 7.4). Die
Profilverwaltung ist damit **kein** Seitenleisten-Punkt — sonst vermischte sich „welcher Server"
mit „was am Server tun".

### 7.2 Globaler Zustand und Sperr-Logik

Die Oberfläche hält einen **`ServerState`** (Online/Offline), der zentral über Freigaben
entscheidet. Der Zustand kommt als Event aus der Crate (7.3) und ist **die** Grundlage der
Sperren:

| Bei **laufendem** Server gesperrt | Grund |
|---|---|
| Aktivierungs-Schalter in G2 | Umschaltung nur bei gestopptem Server (Kap. 7.3 LH) |
| Löschknopf in G2 | Löschen nur gestoppt — online 0 Lösch-Buttons (verifiziert, Kap. 4.3) |
| „Install on server" in G3 | ModHub-Download nur gestoppt (verifiziert, Kap. 7.7 LH) |
| „Speichern" in G6 | `save_settings` fehlt bei laufendem Server (Kap. 7.3 LH) |
| „Starten" im Kopf | bereits online |

| Bei **gestopptem** Server gesperrt | Grund |
|---|---|
| „Stoppen"/„Neu starten" im Kopf | kein laufender Prozess |

Gesperrte Elemente sind **sichtbar deaktiviert mit Begründung** („zum Ändern erst stoppen"),
nicht ausgeblendet — der Nutzer soll verstehen *warum*.

### 7.3 Bibliothek ↔ Oberfläche: Tauri-Schema

- **Commands** (Aufruf GUI → Crate): je Bibliotheks-Operation aus Kap. 4.3 ein Tauri-Command;
  Rückgabe ist `Result<…, AppError>` mit den Fehlertypen aus 4.5.
- **Events** (Strom Crate → GUI): laufende Rückmeldungen ohne Polling:
  - `server_state` — Online/Offline-Wechsel (aktualisiert Statusleiste + Sperren)
  - `log_line` — neue Zeile des laufenden Tails (G5)
  - `progress` — `{ op_id, done, total, label }` für Upload/Download (G3)
  - `op_done` / `op_error` — Abschluss oder Fehler einer langen Operation
- **Abbruch:** Jede lange Operation trägt eine `op_id`; ein `cancel(op_id)`-Command löst das
  `CancellationToken` im `OpCtx` (Kap. 4.2).

### 7.4 G1 — Serverprofile verwalten und verbinden

Das Werkzeug verwaltet **beliebig viele Server** (SZ4, MZ7, F1) — der eigene Docker-Server, ein
gemieteter Server, mehrere Instanzen. Das ist ein Kernnutzen, kein Nebenaspekt.

**Wo im GUI.** Die Profilverwaltung ist ein **eigener Verwaltungsbildschirm** (Overlay/Vollbild),
nicht ein Seitenleisten-Punkt. Erreichbar auf zwei Wegen:
- **Startzustand:** Ist noch kein Server verbunden, zeigt die App direkt diesen Bildschirm
  (Profilliste zum Anlegen/Verbinden) — der natürliche Einstieg.
- **Jederzeit:** über „Server verwalten…" im Server-Dropdown der Statusleiste (7.1).
Das **schnelle Umschalten** zwischen bestehenden Profilen läuft dagegen direkt im Dropdown, ohne
diesen Bildschirm zu öffnen.

**Profilverwaltungs-Bildschirm.**
- **Liste aller Profile** mit Name und Adresse. **Kein Hintergrund-Check** (Entscheidung): Das
  Tool prüft inaktive Profile **nicht** von selbst — es meldet sich nicht ungefragt bei fremden
  Servern an (kein Streuen der Zugangsdaten, kein `loginCount`-Hochzählen, schneller Start).
- **Status nur nach Interaktion:** Nur das Profil, mit dem man sich **verbindet**, zeigt einen
  Live-Status (verbunden / erreichbar / offline / Zugang fehlerhaft — aus dem `connect`).
  Inaktive Profile stehen auf **„nicht verbunden"**. Ein Profil lässt sich per **„Verbinden"**
  (bzw. „prüfen") gezielt einzeln testen.
- **Hinzufügen / Bearbeiten / Löschen / Duplizieren.** Duplizieren hilft bei mehreren
  ähnlichen Instanzen.
- Ein Profil speichert (Typ `ServerProfile`, Kap. 4.2): **Name, Adresse (`http`/`https`),
  Benutzer, Verweis in den Credential-Store, „ungültiges Zertifikat akzeptieren"** (Randfall
  gemietete Server, Kap. 8 LH) sowie **optional den FTP/SFTP-Zugang** (Protokoll, Host, Port,
  Benutzer, eigener Credential-Verweis, mods-Pfad — für große Uploads/Dateizugriff, MZ4).
  **Nie ein Passwort selbst** — beide Zugänge (Web + FTP) gehen direkt in den
  OS-Credential-Store, nicht in ein Feld oder eine Datei der App (Q1/SZ2).
- **Pflichtfelder beim Speichern:** Ein Profil lässt sich nur speichern, wenn der **Web-Zugang
  vollständig** ist — Name, Adresse, Benutzer **und Web-Passwort**. Ist der FTP/SFTP-Zugang
  **aktiviert**, werden auch dessen Felder Pflicht (Host, Port, Benutzer, **FTP-Passwort**,
  mods-Pfad); ohne aktiviertes FTP bleiben sie optional. Fehlt etwas → **„Speichern" gesperrt**
  mit klarer Markierung, kein halbes Profil. Damit ist „verbinden ohne Passwort" im Normalfall
  ausgeschlossen; den Restfall (Credential extern gelöscht / Profil auf anderen Rechner kopiert)
  fängt `CredentialMissing` ab (4.5).
- **Persistenz:** Profil-Datei je OS am Standard-Konfigurationsort; Format und Ablage → Kap. 8
  (Datenhaltung). Zugangsdaten liegen **ausschließlich** im Credential-Store, getrennt von der
  Profil-Datei.

**Aktives Profil und Wechsel.**
- Das **aktive Profil** wählt man in der Statusleiste (7.1); alle Ansichten (G2–G6) beziehen
  sich stets auf dieses eine Profil.
- Beim Wechsel baut die Bibliothek eine **eigene Sitzung** je Server auf (`connect`); ein
  Wechsel meldet den aktuellen Server ordentlich ab bzw. hält Sitzungen getrennt — keine
  Vermischung von Cookies zweier Server.
- **Drei Verbindungszustände**, in der Statusleiste unterscheidbar: **nicht verbunden** (keine
  Sitzung — Startzustand bzw. nach „Verbindung trennen"), **verbunden + gestoppt**, **verbunden
  + läuft**. „Verbindung trennen" (7.1) führt in *nicht verbunden* und zeigt im Arbeitsbereich
  den Verbinden-Bildschirm.

**Verbinden und prüfen (F2).**
- **„Verbindung testen"** ruft `connect`/`state` und zeigt die Kurzinfo (erreichbar /
  angemeldet / erkannte Version) zur Bestätigung.
- **Warnung „öffentlich erreichbares Panel"** (Kap. 7.3 LH), wenn die Bibliothek das meldet.

### 7.5 G2 — Mod-Übersicht

Umsetzung der Soll-Spezifikation aus Kap. 4.1 LH (G2). Konkret:
- **Tabelle** mit Spalten: Auswahl · **Status-Badge** (Aktiv grün / Inaktiv grau /
  **Karteileiche** rot) · Name · Version · Author · **Dateiname (voll)** · Größe · DLC.
- **Suchfeld** (Name + Dateiname), **Filter** (Status/DLC/Author); **Sortierung per Klick auf den
  Spaltenkopf** mit Auf-/Absteigend-Umschaltung und Pfeil-Indikator (kein Dropdown) — alles
  clientseitig über die von `list_mods` gelieferte Liste.
- **Auswahl + Stapelaktion:** „Aktivieren"/„Deaktivieren" sammeln, „Anwenden" ruft `set_active`
  **nach Bestätigung** (7.10).
- **Löschen:** je Mod (bzw. für die Auswahl) ein **Löschknopf** → `delete_mod`. Das **entfernt
  die Mod-Datei vom Server** (destruktiv) → **Bestätigung** (7.10). `delete_mod` wählt intern
  `deleteactive_`/`deleteinactive_` nach Status (Kap. 4.3).
- **Sperre bei laufendem Server** (7.2): Aktivierungs-Checkboxen **und Löschknopf** deaktiviert,
  Banner „zum Ändern erst stoppen"; die Liste bleibt sicht- und durchsuchbar.

### 7.6 G3 — Mods bereitstellen

Zwei Reiter, in der Rangfolge aus F5:
1. **ModHub-Suche** (Weg B, Kann): Suchfeld → `catalog::search` → **Ergebnisliste** (Thumbnail,
   Name, Author, Bewertung). Klick → Detailansicht (`catalog::details`). Knopf **„Install on
   server"** ruft `modhub_download(mod_id)`; Fortschritt über `progress`-Events (7.3), Abbruch
   über die `op_id`. **Die Suche geht jederzeit** (öffentliche Website), aber **„Install on
   server" nur bei gestopptem Server** — bei laufendem Server bietet der ModHub keine Downloads
   an (Kap. 7.7 LH); die GUI zeigt den Knopf dann gesperrt mit Hinweis „zum Installieren erst
   stoppen". Jede Trefferkarte trägt zusätzlich einen **„ModHub"-Knopf**, der die Detailseite
   des Mods (`mod.php?mod_id=<id>`, Kap. 10.2) im **System-Browser** öffnet (Tauri `shell::open`)
   — rein lesend, **jederzeit** verfügbar (auch bei laufendem Server), unabhängig von der
   Install-Sperre.
2. **Datei-Upload:** Dateiwahl → `upload_mod` mit Fortschrittsbalken. Das Tool **wählt den Weg
   selbst:** bis 1,71 GB Web-Formular, **darüber automatisch FTP/SFTP** (MZ4). Ist kein
   FTP/SFTP im Profil hinterlegt und die Datei zu groß (`NoFileAccess`), fordert die GUI auf,
   den FTP/SFTP-Zugang im Profil einzurichten (7.4).

### 7.7 G4 — Serversteuerung (im Kopf, keine eigene Ansicht)

**Entscheidung:** Die Serversteuerung bekommt **keine eigene Seitenleisten-Ansicht** — die drei
Aktionen sitzen direkt in der **Statusleiste**, neben der Zustandsanzeige. Zustand und die eine
Aktion, die ihn ändert, gehören zusammen; ein eigener Menüpunkt nur für drei Knöpfe wäre Overhead.

- **Kontextabhängige Knöpfe** neben dem Status: **gestoppt → „Starten"; laufend → „Neu starten"
  + „Stoppen"** — je nach `ServerState` nur das Sinnvolle (7.2).
- **Alle drei sind eingreifend** → Bestätigungsdialog mit Mitspieler-Hinweis (7.10).
- **Start-Hinweis:** Die Bibliothek führt intern den Voll-Formular-Umlauf aus (Kap. 6); die GUI
  sammelt nichts, weist aber hin „Einstellungen werden unverändert übernommen".
- Nach der Aktion **Ergebnisnachweis** — die belegende Logzeile (`Game server started`/`stopped`)
  wird gemeldet; für Details bleibt die Log-Ansicht (G5, Q3).

> **Zurückgestellt: Live-Status.** Uptime, Spieler online (Name/Spielzeit) und RAM aus dem
> Statistik-Block (Kann-Ziel, Lastenheft 1.3) sind **nicht in v1**. Ohne Steuerungs-Ansicht
> bräuchten sie einen eigenen Ort — das wird später entschieden. CPU bleibt ausgenommen (nur
> Grafik, kein Zahlwert).

### 7.8 G5 — Log

- **Logdatei-Auswahl** (`list_logs`) und fortlaufender Strom über `log_line`-Events (`tail`).
- **Fehler hervorgehoben** (z. B. `missing dependencies`), Autoscroll mit Pause-beim-Hochscrollen.
- Diese Ansicht ist der Ort, an dem eingreifende Aktionen ihren **Nachweis** zeigen (Q3).

### 7.9 G6 — Spieleinstellungen

Menüpunkt **„Spieleinstellungen"** (ein Wort, wie im UI), im Seitenmenü an erster Stelle.
Umsetzung aus Kap. 4.1 LH (G6):
- **Gruppen:** Identität · Spielwelt · Netzwerk & Zugang · Automatik.
- **Passwörter maskiert mit „Anzeigen"-Knopf** (Umschalter), nie im Log (Q1).
- **`stats_interval` menschenlesbar** (z. B. „31536000 s ≈ 365 Tage — Feed praktisch aus").
- **Bearbeiten/Speichern nur bei gestopptem Server** (7.2); sonst nur-lesend mit Hinweis.
- **Neues-Spiel-Felder nur bei leerem Savegame:** Map, Startgeld, Kredit und Schwierigkeit
  (`map_start`/`initialMoney`/`initialLoan`/`economicDifficulty`) sind nur editierbar, wenn ein
  **leerer** Savegame-Slot gewählt ist; bei einem bestehenden Spielstand zeigt die GUI sie
  gesperrt — genau wie der Server (`checkSavegame()`, Kap. 6).
- Vor `save_settings` **Feldwerte prüfen** (Wertebereiche) → sonst klare Meldung (Q4).

### 7.9a G7 — Savegames (Kann, Kap. 1.3 LH / 7.8 LH)

Eigener Menüpunkt **„Savegames"**, an das Original angelehnt (drei Aufgaben, Kap. 7.8 LH), aber
statt untereinander gestapelt in **drei Reiter** getrennt — gleiches Muster wie G3 (7.6):

1. **Reiter „Manage Savegames"**
   - **Tabelle** der belegten Slots (`list_savegames`): Slot-Name · Map · Geld · Spielzeit ·
     Schwierigkeit.
   - Je Zeile **„Herunterladen"** → `download_savegame` (Dateiauswahl-Dialog für den Zielpfad,
     Fortschritt über `ctx`).
   - Je Zeile **„Löschen"** → `delete_savegame` — **destruktiv** → Bestätigung (7.10). **Fehlt
     beim aktuell geladenen Savegame** (live verifiziert: dessen Zeile trägt server­seitig keinen
     Lösch-Link — `ServerSavegame.can_delete = false`); die GUI zeigt dort statt des Knopfs einen
     Hinweis („aktiv") statt eine Aktion anzubieten, die ohnehin fehlschlagen würde.
2. **Reiter „Upload Savegame"**
   - **Ziel-Slot-Auswahl** (Dropdown) — **die echten Optionen des `index_upload`-Formulars**
     (`list_savegame_upload_slots`), **nicht** synthetisch 1..20 nachgebaut: belegte Slots mit
     ihrem Anzeigenamen, leere als „SAVEGAME n", **ohne** das aktuell geladene Savegame — live
     verifiziert, dass genau dieser Slot im Original-Dropdown fehlt (man kann ihn nicht
     überschreiben, während er läuft). Dazu optionaler **Name** (`custom_name`), **Datei-Upload**
     (ZIP) → `upload_savegame` mit Fortschrittsbalken.
   - Wie bei G3: Das Tool **wählt den Weg selbst** (bis 1,71 GB Web-Formular, darüber FTP/SFTP,
     `NoFileAccess` falls kein Zugang hinterlegt, 7.4).
   - Wird ein **belegter** Slot als Ziel gewählt, weist die GUI vor dem Absenden darauf hin, dass
     der bestehende Spielstand **überschrieben** wird — analog zur Restore-Warnung unten.
3. **Reiter „Restore Savegame Backup"**
   - **Slot wählen**, dann **Dropdown der Zeitstempel-Backups** dieses Slots (`list_savegame_backups`)
     mit Map/Spielzeit zur Orientierung (wie im Original-Formulartext).
   - **„Wiederherstellen"** → `restore_savegame_backup` — **überschreibt den Slot** →
     **Bestätigung** (7.10) mit explizitem Hinweis „Der aktuelle Spielstand in diesem Slot geht
     verloren."

**Keine Sperre bei laufendem Server.** Anders als G2 (Aktivierung/Löschen) und G6 (Speichern)
bleiben alle drei Reiter **immer** bedienbar — verifiziert am lebenden Server: Upload, Löschen
und Backup-Restore funktionieren unabhängig vom Serverzustand, genau wie `upload_mod` (7.6).
Kein „zum Ändern erst stoppen"-Banner in G7.

### 7.10 Bestätigungen bei eingreifenden Aktionen

Ein einheitlicher Bestätigungsdialog für: **Stoppen/Neustart, Aktivierungsänderungen, Start,
Mod löschen, Savegame löschen, Savegame-Backup wiederherstellen** (Q2). Er nennt die konkrete
Folge („Der Server wird gestoppt — verbundene Mitspieler fliegen heraus." bzw. „Die Mod-Datei
wird vom Server gelöscht." bzw. „Der aktuelle Spielstand in diesem Slot geht verloren.") und
verlangt aktive Zustimmung. Harmlose Aktionen (Suche, Log lesen, Sortieren, Herunterladen)
brauchen keine.

### 7.11 Fehleranzeige

Die Fehlertypen aus 4.5 werden auf **verständliche Meldungen** abgebildet (nicht der rohe
Fehler): `FormMismatch` → „Serverformat nicht erkannt — Aktion abgebrochen"; `ServerRunning`
→ „zum Ändern erst stoppen"; `NotProven` → „Aktion abgesetzt, aber im Log nicht bestätigt".
Grundsatz Q3: **lieber ehrlich unbestätigt als falsch ‚fertig'.**

---

## 8. Datenhaltung

### 8.1 Was persistiert wird
- **Server-Profile** (Kap. 4.2) — **ohne Passwort**.
- **Anwendungseinstellungen** — zuletzt aktives Profil, Theme, Sprache.
- **Nicht** persistiert: Passwörter (→ Credential-Store, 8.4), Sitzungs-Cookies (nur im
  Speicher), Mod-Listen/Logs/Serverzustand (immer frisch vom Server, 8.6).

### 8.2 Format
**JSON über `serde`** (Gleichklang mit ModMatcher). Menschlich lesbar und notfalls von Hand
editierbar. Jede Datei trägt ein **`schema_version`**-Feld für spätere Migration.

### 8.3 Ablageort je Betriebssystem
Standard-Konfigurationsverzeichnis, ermittelt über die `directories`-Crate (nicht geraten):

| OS | Verzeichnis |
|---|---|
| Windows | `%APPDATA%\ServerControl\` (Roaming) |
| macOS | `~/Library/Application Support/ServerControl/` |
| Linux | `$XDG_CONFIG_HOME/servercontrol/` bzw. `~/.config/servercontrol/` |

Dateien: `profiles.json`, `settings.json`. Unter Linux läuft das Tool ohne Sandbox (AppImage,
analog ModMatcher-PH 4.4) → der normale XDG-Pfad ist erreichbar.

### 8.4 Trennung Profil ↔ Credential-Store (Q1/SZ2)
- Die Profil-Datei enthält nur einen **Verweis** (`credential_key`), **nie** das Passwort.
- Das Passwort liegt im OS-Credential-Store über `keyring`: **service** = `"servercontrol"`,
  **account** = eine **stabile Profil-UUID**. Bewusst **nicht** URL/Benutzer als Schlüssel —
  sonst verwaist der Eintrag, sobald man Adresse oder Benutzer eines Profils ändert.
- **Zwei Zugänge je Profil:** Web-Login und FTP/SFTP haben getrennte Passwörter → **zwei**
  Einträge, unterschieden über das Account-Suffix (z. B. `<uuid>/web` und `<uuid>/ftp`).
- **Löscht** man ein Profil, werden **beide** zugehörigen Credential-Einträge **mitentfernt**.
- Zu **keinem** Zeitpunkt landet ein Passwort in Datei, Log oder temporärer Datei (Q1).

### 8.5 Robustheit
- **Atomares Schreiben:** erst in eine temporäre Datei, dann umbenennen — kein halb
  geschriebener Zustand bei Absturz/Stromausfall.
- **Dateirechte:** nur für den Nutzer lesbar (`0600`-artig), auch ohne Geheimnisse in der Datei
  — Hygiene.
- **Schema-Migration:** unbekannte künftige Felder tolerieren; eine **höhere** `schema_version`
  klar melden, statt Daten zu überschreiben oder zu verlieren.
- **Fehlende/kaputte Datei:** leerer Startzustand (Profilliste leer, Verwaltungsbildschirm 7.4),
  keine harte Fehlermeldung.

### 8.6 Was bewusst *nicht* hier liegt
- **Mod-Sets** → serverseitig über FTP/SFTP (Lastenheft 1.3, spätere Ausbaustufe) — nicht in der
  lokalen Datenhaltung.
- **Live-Daten** (Mod-Liste, Log, Serverzustand) werden **nicht** zwischengespeichert, sondern
  bei Bedarf frisch vom Server geholt — verhindert veraltete Anzeige (Geist von Q3).

---

## 9. Ergebnis-Verifikation (Q3)

Grundsatz: **kein „fertig" aus einer HTTP-200-Antwort.** Der POST bestätigt nur die Annahme,
nicht die Wirkung. Der Nachweis kommt aus zwei Quellen — beobachtbarer Zustand (primär) und Log
(Gründe).

### 9.1 Zwei Signalquellen
- **Beobachtbarer Zustand (primär, robust):** Die Home-Seite kodiert den Zustand **mehrfach**
  (am lebenden Server in beiden Zuständen verifiziert):

  | Signal | offline | online |
  |---|---|---|
  | **`div.status-indicator`** (primärer Anker) | Klasse `offline` | Klasse `online` |
  | Text im `<span>` | `OFFLINE` | `ONLINE` |
  | `configuration`-Buttons | `save_settings`+`start_server` | `stop_server`+`restart_server` |
  | Mod-Checkboxen (`ActiveMods`/`InactiveMods`) | vorhanden | 0 |
  | Statistik-Block (Uptime/Spieler/RAM) | fehlt | vorhanden |

  Der **primäre Anker ist `div.status-indicator`** mit Modifier `online`/`offline` — semantisch,
  stabil. Zusätzlich ändert sich die Mod-Liste bei Upload/Löschen. Diese Signale haben **kein
  Offset-/Rotations-Problem** — sie sind der eigentliche Beweis.
- **Server-Log (ergänzend):** liefert den **Grund** eines Fehlschlags (z. B.
  `missing dependencies`), den der Zustand allein nicht zeigt, plus den Zeitpunkt (Kap. 7.4 LH).

### 9.2 Verifikation je Aktion

| Aktion | Primär-Beweis (Zustand) | Log (Detail / Fehler) |
|---|---|---|
| **Start** | Home-Seite wird ONLINE, Serverstatistik erscheint | `Game server started`; Fehler wie `missing dependencies` |
| **Stopp** | Home-Seite wird OFFLINE (Statistik weg, Buttons wechseln) | `Game server stopped` |
| **Neustart** | Übergang OFFLINE → ONLINE | beide Marken |
| **Mod aktivieren/deaktivieren** | Mod **wechselt den Bereich**: Dateiname erscheint danach unter dem gegenteiligen Formular (siehe unten) | (nur bei gestopptem Server möglich) |
| **Upload** | Mod erscheint in der Liste | `Uploaded mod '<Datei>'` |
| **Löschen** | Mod verschwindet aus der Liste | `Mod '<…>' deleted` |
| **ModHub-Download** *(nur gestoppt)* | Fortschritt bis `total`, danach Mod in der Liste | — |

**Zur Mod-Umschaltung im Detail:** Die Home-Seite trennt aktive und inaktive Mods in zwei
Formulare (Kap. 7.3 LH) — `ActiveMods` (Checkboxen `moddeactivate_<Datei>`) und `InactiveMods`
(`modactivate_<Datei>`). Ein erfolgreicher Wechsel zeigt sich strukturell: **derselbe Dateiname
steht nach dem erneuten Lesen unter dem *gegenteiligen* Formular** — aktiviert: von
`modactivate_` nach `moddeactivate_`; deaktiviert umgekehrt. `list_mods` liefert diesen
Bereichswechsel als `ModStatus::Active`/`Inactive` — kein Rätselraten, sondern ein eindeutiges
Signal.

### 9.3 Ablauf: pollen bis Ergebnis oder Timeout
- Nach der Aktion wird der **Zustand in Intervallen** abgefragt (Home-Seite/`state`, `list_mods`),
  bis das erwartete Ergebnis eintritt **oder** das Zeitfenster abläuft.
- **Zeitfenster je Aktion:** Start großzügig (der Spielprozess braucht spürbar Zeit), Stopp knapp.
- **Timeout →** die Bibliothek meldet `NotProven` und zeigt das **Log im Klartext** — ehrlich
  unbestätigt statt falsch „fertig" (Q3, Fehleranzeige 7.11).

### 9.4 Das Log als Fehlerquelle
- Für den **Grund** eines Fehlschlags wird immer das Log herangezogen (fehlende Abhängigkeiten
  usw.), auch wenn der Zustand-Poll schon „nicht gestartet" sagt.
- **Logrotation:** Jeder Serverstart legt eine **neue** Logdatei mit Zeitstempel an (Kap. 10.4
  LH). Beim „Start" wird die Erfolgs-/Fehlermarke daher in der **neuen** Datei gesucht, nicht in
  der zuvor gelesenen. Offset/Rotation betreffen damit nur diesen ergänzenden Detail-Blick —
  nicht mehr den Primärbeweis.

---

## 10. ModHub-Parser

Zwei getrennte Parser, weil **zwei Hosts:** (A) die **Server-Seiten** (`mods.html`) und (B) die
**öffentliche ModHub-Website** (`farming-simulator.com`). Beide liefern **gerendertes HTML ohne
API** — Robustheit gegen Layout-Änderungen ist Pflicht (SZ1/Q4). Alle Angaben sind am lebenden
Server bzw. an der Website verifiziert (Kap. 7.7 LH).

> **Rohes HTML parsen, nicht das gerenderte DOM.** Felder hinter „Show more"
> (`toggleGridElement`) sind im **ausgelieferten HTML vollständig vorhanden** — nur visuell
> ausgeblendet. Der Parser liest die Server-Antwort, nicht den sichtbaren Zustand.

### 10.1 Server-Kategorieseiten (A)
- **URL:** `GET mods.html?category=<id>&lang=en&page=<p>`. **Nur angemeldet** — ohne Login
  liefert `?category=` die *installierten* Mods (Kap. 7.7 LH). Der Parser prüft deshalb, dass er
  auf einer ModHub-Kategorie ist (Vorhandensein von `startmoddownload`-Buttons), sonst
  `ParseError`/`NotLoggedIn`.
- **Kategorie-IDs** (Dropdown-Werte): 0 DLC · 1 All · 3 Update · 5 Latest · 6 Best ·
  7 Most Downloaded · 8 Package · 9 Official Mods · 10–13 Map (EU/NA/SA/other) · 14 Gameplay.
- **Seitengröße:** 250 Einträge/Seite; **Pagination** über `page=<n>` (Blätter-Links).
- **Ein Eintrag** — Anker und Felder:
  - **Anker** = `<button name="startmoddownload" value="<mod_id>" title="Install <Dateiname>">`
    → liefert **`mod_id`** *und* **Dateiname** (aus dem `title`).
  - **Metadaten** als `<b>Label</b>` + `<i>Wert</i>`-Paare: Name, Version, Author, Filename,
    Size, Deps (Issues, Hub, Active als Flags). **An den Labels ankern**, nicht an CSS-Klassen.
- **Fortschritts-Element:** `.mod-download-progress` mit Attribut `modid` (= `mod_id`) — der
  Client pollt `mods.html?modhubdownloadprogress=<mod_id>` → JSON `{downloaded, total}` (Kap. 7.7 LH).

### 10.2 Öffentliche ModHub-Website (B) — Suche und Detail
- **Suche:** `GET farming-simulator.com/mods.php?title=fs2025&searchMod=<query>` — Textfeld
  **`searchMod`**, verstecktes `title=fs2025` wählt das Spiel FS25.
- **Ergebnisliste:** Karten mit `a[href*="mod_id="]`; je Karte **`mod_id`** (aus dem `href`),
  Name, Author, Bewertung, Thumbnail. Der erste Eintrag „**FEATURED MOD**" ist **Werbung** und
  wird verworfen (nicht als Treffer zählen).
- **Detailseite:** `mod.php?mod_id=<id>` → Name, Author, Version, Dateiname, Beschreibung.
- **Brücke:** `mod_id` (Website) **=** `startmoddownload`-ID (Server) — verifiziert an `366506`
  (Kap. 7.7 LH). „Install on server" reicht die `mod_id` direkt an `modhub_download` (Kap. 4.4).

### 10.3 Stabile Anker vs. volatile Struktur (SZ1/Q4)
| Stabil — **daran** ankern | Volatil — **nicht** ankern |
|---|---|
| `name="startmoddownload"`, `value=<id>`, `title` | CSS-Grid-Klassen (`col col-lg-3 …`) |
| `modhubdownloadprogress=`, Attribut `modid` | `toggle-element-<zahl>` |
| `mod_id=` im `href`, `searchMod=` | Reihenfolge/Verschachtelung der `div`s |
| die `<b>Label</b>`-Texte | Whitespace, Zeilenumbrüche |

Fehlt ein erwarteter Anker → **`ParseError`** mit klarer Meldung, statt Halb-Daten
auszuliefern. Ein Prüfskript in `tools/` vergleicht die Anker gegen gesicherte echte Stände
(Regressionsschutz, Kap. 11).

### 10.4 Sprache fixieren
Beim Server-Parsen `lang=en` erzwingen, damit die `<b>Label</b>`-Texte (Name/Version/…) stabil
englisch sind — **unabhängig von der GUI-Sprache**. Sonst brächen lokalisierte Labels den
Parser. Analog für die Website (`lang`/`country`).

---

## 11. Test- und Abnahmeplan (Zuordnung zu A1–A6)

### 11.1 Testebenen
- **Parser-Unit-Tests gegen Fixtures** (11.3): die Parser (Kap. 10, plus Home-/Mods-/Login-/
  Log-Parser) werden gegen **gesicherte echte HTML-Stände** geprüft — ohne laufenden Server.
- **Crate-Integrationstests gegen einen Attrappen-Server** (Mock-HTTP, das die Fixtures
  ausliefert): Sitzung, Formular-Umlauf, Verifikationslogik (Kap. 9), Fehlerpfade.
- **Formular-Abgleich gegen den echten Server** (Prüfskript in `tools/`): vergleicht die
  erwarteten Feldnamen/Anker (Kap. 6, 10.3) mit einem realen Stand — Regressionsschutz für die
  Versionstoleranz (SZ1/Q4).
- **Manuelle GUI-Tests** je Plattform (11.4).

### 11.2 Abnahmefälle A1–A6

| # | Kriterium (Lastenheft) | Testfall | Nachweis |
|---|---|---|---|
| **A1** | Verbinden, Mod-Liste sehen | `connect` gegen Testserver → `list_mods` | Liste mit erwarteter Anzahl/Status in G2 |
| **A2** | Mod aktivieren, danach sichtbar | bei **gestopptem** Server `set_active([x])` | `x` wechselt von `modactivate_` nach `moddeactivate_` (Kap. 9.2); Status in G2 + Weboberfläche |
| **A3** | Stoppen/Starten, im Log belegt | `stop` → `start` | Zustand OFFLINE/ONLINE (Kap. 9) **und** Logmarke `Game server stopped`/`started` |
| **A4** | Datei hochladen, erscheint in Liste | `upload_mod(datei)` | Mod in `list_mods`; Log `Uploaded mod '<Datei>'` |
| **A5** | „Mod-Satz herstellen" mit nachgewiesenem Ergebnis | **v1: manuelle Folge** über G2/G3/G4 (stoppen → bereitstellen/aktivieren → starten); F8-Assistent zurückgestellt (Lastenheft 4.1) | am Ende ONLINE **und** belegende Logzeile (Kap. 9.3) |
| **A6** | Bei unerwartetem Aufbau abbrechen statt raten | Fixture mit **verändertem/fehlendem** Formularfeld → Schreib-Operation | `FormMismatch`, **kein** POST; Meldung „Serverformat nicht erkannt" (Kap. 6, 7.11) |

### 11.3 Fixtures (Aufnahme echter Stände)
Ein Skript in `tools/` sichert die realen Antworten des Testservers als Testdaten:
Login-Seite · Home **online** und **offline** · Mods-Seite · ModHub-Kategorieseite ·
Website-Suche + Detailseite · Beispiel-Logzeilen (Erfolg **und** Fehler).

> ⚠️ **Zugangsdaten aus Fixtures entfernen (Q1):** Das `configuration`-Formular enthält
> `admin_password`/`game_password` als **Klartextfelder** (Kap. 7.3 LH). Beim Sichern werden
> diese Werte **maskiert/entfernt**, damit keine Passwörter in Testdaten oder ins Repo geraten.

### 11.4 Plattform-Verifikation (Q5)
- **CI-Build-Matrix** Windows/macOS/Linux (analog ModMatcher-PH 4.2).
- **`keyring` je OS prüfen:** bindet an unterschiedliche Stores (Windows Credential Manager,
  macOS Keychain, Linux Secret Service) — je Plattform ein Test „speichern/lesen/löschen".
- **`http`/`https`** inkl. Randfall selbstsigniertes Zertifikat (Lastenheft Kap. 8) prüfen.
- **FTP/SFTP-Upload** einer Datei >1,71 GB gegen den Testserver (MZ4), inkl. Fortschritt und
  Weg-Auswahl Web↔FTP nach Größe.
- **GUI (Tauri)** je Plattform manuell (Sperr-Logik laufend/gestoppt, Log-Strom, Fortschritt).

### 11.5 Sicherheits-Checks
- **Kein Passwort** in Logs, Fehlermeldungen, temporären Dateien oder Fixtures (Q1) —
  automatisierter Suchlauf über erzeugte Ausgaben als Teil der CI.
- **Warnung „öffentliches Panel"** wird im passenden Fall ausgelöst (Kap. 7.3 LH / G1).

---

## 12. Kapitelstand

Alle für den Entwurf 0.1 vorgesehenen Kapitel sind ausgearbeitet:

- [x] **GUI-Spezifikation G1–G7** (Kap. 7; G7 Savegames = Kann-Ziel, Kap. 7.9a)
- [x] **Profil-/Datenhaltung** (Kap. 8)
- [x] **Ergebnis-Verifikation** — Zustand primär, Log für Gründe (Kap. 9)
- [x] **ModHub-Parser** — stabile Anker, zwei Hosts, Sprache fixiert (Kap. 10)
- [x] **Test- und Abnahmeplan** — A1–A6, Fixtures, Plattform-Verifikation (Kap. 11)

> **Entfällt:** „Einbindungs-Schnittstelle für ModMatcher" — die Kopplung ist als *optional,
> jetzt nicht entworfen* entschieden (Kap. 2.4). Wird erst ausgearbeitet, falls ModMatcher
> tatsächlich eingreifende Server-Aktionen anbietet.

**Nächster Schritt (außerhalb dieses Entwurfs):** Umsetzung — Projektgerüst im neuen Repo
`servercontrol` (Kap. 3), beginnend mit `servercontrol-core` (Sitzung, Mods, Steuerung) gegen
die gesicherten Fixtures.
