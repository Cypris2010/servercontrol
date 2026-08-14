# Lastenheft: Server Control for Farming Simulator 2025

**Version:** 0.1 (Entwurf)
**Datum:** 2026-07-22
**Auftraggeber:** Fabian Bott
**Verhältnis zu ModMatcher:** eigenständiges Werkzeug. Die Bibliothek-zuerst-Architektur hält
die Tür offen, dass ModMatcher sie *optional* später als Bibliothek einbindet — diese Kopplung
ist aber **nicht vorausgesetzt** und wird jetzt nicht entworfen (Pflichtenheft Kap. 2.4). Beide
dienen verschiedenen Rollen: ModMatcher dem Client-Abgleich, Server Control der Server-Verwaltung
(siehe [Lastenheft.md](Lastenheft.md)).

Das Dokument ist geschichtet:
- **Teil A – Lastenheft** (das *Was*): Kap. 1–6
- **Teil B – Verifiziertes technisches Fundament** (das *Wie es wirklich funktioniert*): Kap. 7
- **Teil C – Anhang** (offene Punkte, Beweislage): Kap. 8–9

---

# Teil A – Lastenheft

## 1. Zielbestimmung

Ein Werkzeug zur **Fernsteuerung eines FS25-Dedicated-Servers über dessen eigene
Weboberfläche**. Es automatisiert die Klickarbeit, die Admins heute von Hand erledigen —
insbesondere das Aktivieren von Mods, das Hochladen und das Starten/Stoppen des Servers.

**Warum eigenständig:** Server-Verwaltung unterscheidet sich grundlegend vom
Client-Abgleich — anderes Risiko (fremder Server statt eigener Ordner), andere Rechte
(Admin-Zugang), andere Stabilität (undokumentierte Herstelleroberfläche). Sie gehört
deshalb nicht in den ModMatcher-Kern.

### 1.1 Muss-Ziele
- **MZ1** Mod-Liste des Servers lesen (aktiv / inaktiv / vorhanden).
- **MZ2** Mods **aktivieren und deaktivieren**.
- **MZ3** Server **starten, stoppen, neu starten**.
- **MZ4** Mods auf den Server **hochladen** — über das Web-Formular und, für Dateien über
  1,71 GB, über **FTP/SFTP**. FTP/SFTP-Support ist damit fester Bestandteil, kein Kann.
- **MZ5** Server-**Logs mitlesen** (fortlaufend).
- **MZ6** Nutzbar als **Bibliothek** mit klarer Schnittstelle; **CLI und GUI** sind
  gleichberechtigte, dünne Schichten darauf (siehe Kap. 4).
- **MZ7** Funktioniert mit **fremden/gemieteten Servern**, nicht nur mit eigenen.

### 1.2 Soll-Ziele
- **SZ1 Versionstoleranz:** erkennen, wenn die Weboberfläche nicht mehr wie erwartet
  aufgebaut ist, und **abbrechen statt blind zu handeln**.
- **SZ2 Zugangsdaten** im OS-Credential-Store, niemals im Klartext oder in Protokollen.
- **SZ3 Ergebnisprüfung:** Erfolg einer Aktion **am Log nachweisen**, nicht annehmen.
- **SZ4** Mehrere Server-Profile verwalten.

### 1.3 Kann-Ziele
- **Savegames hoch-/herunterladen, auswählen, löschen und aus Backup wiederherstellen** — läuft,
  wie bei Mods, primär über das **native Web-Formular** der Server-Oberfläche
  (`savegames.html`, Kap. 7.8), **nicht** über FTP/SFTP: Download ist ein einfacher HTTP-Link je
  Slot, Upload ein `multipart/form-data`-POST. FTP/SFTP wäre nur nötig, falls eine
  Savegame-ZIP-Datei die 1,71-GB-Grenze (Kap. 7.3) überschreitet — bei Savegames praxisfern,
  aber vom selben Mechanismus wie bei Mods (F5) mitabgedeckt.
- Allgemeiner Dateizugriff auf den Serverordner über FTP/SFTP (Dateien ablegen/lesen/holen) —
  trägt später auch die Mod-Sets
- Server-Einstellungen ändern (Name, Passwörter, Slots, Karte)
- **Server-Statusanzeige** (nur bei laufendem Server, aus dem Statistik-Block der Home-Seite):
  **Uptime, Spieler online** (mit Name/Spielzeit), **RAM-Auslastung** (34 % / 2,62 GB),
  Map/Slots. Alles als Text auslesbar. **CPU ausgenommen** — dort gibt es nur eine
  Verlaufsgrafik ohne Zahlwert (SVG-Linie), die zu interpretieren sich nicht lohnt.
- ModHub-Downloads anstoßen und deren Fortschritt verfolgen
- **ModHub-Suche per Name im Tool:** Namenssuche über die öffentliche ModHub-Website, eigene
  Ergebnisliste mit Mod-Infos, „Install on server" löst den serverseitigen Download aus
  (Weg B, Kap. 7.7). Die Server-Oberfläche selbst bietet **keine** Suche.
- **Mod-Sätze (Mod-Sets) — spätere Ausbaustufe, nicht in der ersten Version.** Benannte
  Zusammenstellungen von Mods, die als Satz auf den Server angewendet werden: die Mitglieder des
  Satzes werden aktiv geschaltet, alle übrigen inaktiv. Anwenden geschieht bei gestopptem Server
  (Kap. 7.3). Abzugrenzen vom **Server-Profil** (welcher Server) — ein Mod-Set beschreibt
  *welche Mods aktiv* sind.
  - **Speicherung ausschließlich serverseitig über FTP/SFTP** (Entscheidung). Folge: Das
    Feature setzt FTP/SFTP-Zugang voraus und ist ohne diesen nicht verfügbar.
  - Ein Set speichert je Mod **Dateiname + optional die ModHub-ID**, damit fehlende Mods vor dem
    Anwenden nachgeladen werden können (F5), statt zu scheitern (`InactiveMods` listet nur
    vorhandene Dateien).
- Benutzerverwaltung, Journal

### 1.4 Abgrenzung (Nicht-Ziele)
- **Kein direktes Bearbeiten von `dedicatedServerConfig.xml`** — nachweislich wirkungslos
  (Kap. 7.1).
- **Kein direktes Schreiben in die SQLite-Datenbank** — undokumentiertes Herstellerinterna
  (bewusste Entscheidung, Kap. 7.2).
- **Keine Spielerverwaltung** (kick/ban/chat) — die Weboberfläche bietet das nicht.
- Kein Hosten oder Bereitstellen von Mods.

---

## 2. Zielgruppen
- **Server-Admins** mit eigenem oder **gemietetem** Server.
- **ModMatcher** als programmatischer Nutzer der Bibliothek.

---

## 3. Produktfunktionen

- **F1 Server-Profil verwalten:** Adresse (mit Protokoll **`http` oder `https`**), Port,
  Admin-Zugangsdaten (im Credential-Store). Das Schema ist der einzige zu erwartende Unterschied
  zu gemieteten Servern und für die Umsetzung folgenlos (Kap. 8).
- **F2 Verbindung prüfen:** Erreichbarkeit, Anmeldung, erkannte Server-Version.
- **F3 Mod-Liste lesen:** aktive und inaktive Mods samt Kennung, Version, Hash, DLC-Flag.
- **F4 Mods aktivieren/deaktivieren** über das Formular der Weboberfläche.
- **F5 Mods bereitstellen** — in dieser Rangfolge:
  1. **ModHub-Download durch den Server** (`startmoddownload`), wenn der Mod dort verfügbar
     ist. Der Server holt ihn über seine eigene Anbindung — kein Upload von zu Hause, keine
     Größenbeschränkung. **Bevorzugter Weg.**
  2. **Formular-Upload** (`modUpload`) für eigene Mods bis 1,71 GB.
  3. **FTP/SFTP** für eigene Mods **darüber** — fester Weg (Muss, MZ4), da der Web-Upload bei
     1,71 GB endet. Das Tool wählt bei zu großen Dateien automatisch diesen Weg.
- **F5a ModHub durchsuchen** (Kann-Ziel, siehe 1.3) — die Server-Oberfläche bietet **keine
  Suche**, nur ein Kategorie-Dropdown mit 250 Einträgen/Seite (verifiziert, Kap. 7.7).
  ServerControl bietet stattdessen eine **Namenssuche über die öffentliche ModHub-Website**
  (`searchMod=`, **Weg B**) mit eigener Ergebnisliste; ein Klick „Install on server" löst den
  Download über F5 aus (`startmoddownload=<mod_id>`). Der Weg über einen **lokalen Index** aller
  Kategorieseiten (Weg A) wurde verworfen — bei tausenden Mods zu datenaufwendig. Der
  Download-Mechanismus des Servers bleibt unangetastet, nur die Suche wird ergänzt.
- **F6 Server starten / stoppen / neu starten.**
- **F7 Log mitlesen** — fortlaufend, mit Auswahl der Logdatei.
- **F8 Ablauf „Mod-Satz herstellen"** — der eigentliche Nutzen, weil die Einzelschritte
  zusammenhängen:
  ```
  stoppen  →  Dateien hochladen  →  Seite lesen (Erkennung greift dabei)
           →  modactivate_<Dateiname>  →  starten  →  Ergebnis im Log prüfen
  ```
  **Kein zusätzlicher Neustart nötig:** Der Server registriert neu abgelegte Dateien beim
  nächsten Seitenaufruf selbst (Kap. 8).
- **F9 Sicherheitsabfragen:** eingreifende Aktionen (Stoppen, Aktivierungsänderung) nur
  nach ausdrücklicher Bestätigung; Hinweis, dass Mitspieler betroffen sein können.

---

## 4. Aufbau

```
                    ┌──▶  CLI   (Skripte, Server ohne Oberfläche, Tests)
   Bibliothek  ─────┤
   (Kernlogik)      └──▶  GUI   (Admins am Rechner)
```

**Die Bibliothek ist der einzige Ort mit Logik.** CLI und GUI sind gleichberechtigte,
dünne Schichten — die GUI ruft **nicht** die CLI auf. Begründung: Typsicherheit,
saubere Fehlerbehandlung und Rückmeldungen im Sekundentakt (Log, Upload-Fortschritt)
gehen über eine Kommandozeilen-Zwischenschicht verloren.

**Optional** kann ModMatcher später dieselbe Bibliothek einbinden, statt ein zweites Programm
fernzusteuern — vorausgesetzt ist das aber nicht (Pflichtenheft Kap. 2.4).

### 4.1 Was die GUI leisten soll

Die GUI ist für **Admins am Rechner**. Ihr Zweck ist nicht, die Weboberfläche des Servers
nachzubauen, sondern deren **verifizierte Eigenheiten sichtbar und beherrschbar** zu machen —
insbesondere die Dinge, an denen die Hersteller-Oberfläche schweigt (Karteileichen,
Klartext-Passwörter, fehlende Erfolgsmeldung). Sie ruft ausschließlich die Bibliothek auf.

Dieser Abschnitt beschreibt das *Was* (Ansichten und ihr Zweck). Die konkrete Umsetzung
(Widget-Bibliothek, Layout) gehört ins Pflichtenheft.

**Getrennte Ansichten statt einer Seite.** Die Hersteller-Oberfläche legt **Spiel-Einstellungen**
(Name, Passwörter, Savegame, Map, Geld, Port, Slots, Sprache, Intervalle, Pause, Crossplay) und
die **Mod-Liste** auf *eine* Seite (`index.html`). Das ist deren Layout, kein technischer Zwang:
Unser Werkzeug redet über die Bibliothek mit einzelnen Endpunkten, nicht mit „einer Seite". Es
sind zwei getrennte Aufgaben — **Mod-Verwaltung** (G2) und **Spiel-Einstellungen** — und die GUI
trennt sie in eigene Ansichten, dazu die **Serversteuerung** (G4). Gemeinsame Voraussetzung, die
alle drei teilen: **Einstellungen speichern und Mods umschalten geht nur bei gestopptem Server**
(Kap. 7.3) — die getrennten Ansichten dürfen nicht den Eindruck erwecken, im laufenden Betrieb
sei frei daran zu drehen.

**Leitgedanken** (aus den Qualitätsanforderungen abgeleitet):
- **Nichts behaupten, was nicht belegt ist.** Das Ergebnis einer Aktion stammt aus dem Log,
  nicht aus einer Annahme — und wird dort sichtbar gemacht (Q3).
- **Eingriffe kenntlich machen.** Stoppen, Starten und Aktivierungsänderungen sind spürbare
  Eingriffe in einen ggf. bespielten Server; sie sind optisch abgesetzt, bestätigungs­pflichtig
  und weisen darauf hin, dass Mitspieler betroffen sein können (Q2).
- **Passwörter niemals zeigen oder speichern.** Zugangsdaten kommen aus dem Credential-Store,
  laufen bei Start/Einstellungen als Klartextfelder durch die Bibliothek, tauchen in der GUI
  aber weder als Anzeige noch im Log auf (Q1).
- **Bei unerwartetem Aufbau abbrechen, nicht raten.** Passt die Weboberfläche nicht zum
  erwarteten Formularaufbau, zeigt die GUI eine klare Meldung statt blind zu handeln (Q4).

**G1 Serverauswahl und Verbindungsstatus.** Auswahl unter mehreren Profilen (SZ4); sichtbarer
Zustand: erreichbar / angemeldet / erkannte Server-Version. Deutliche Warnung, wenn das Panel
**öffentlich erreichbar** ist (fehlendes CSRF-Token, Klartext-Passwortfelder — Kap. 7.3).

**G2 Mod-Übersicht — die zentrale Ansicht.** So soll sie sein:

- **Eine einzige Tabelle** über den gesamten Bestand — aktive, inaktive und Karteileichen
  gemeinsam, nicht in getrennte Blöcke gespalten.
- **Spalten:** Auswahl (Checkbox) · Status · Name · Version · Author · **Dateiname (vollständig,
  nicht abgeschnitten)** · Größe · DLC. Der Dateiname ist die Kennung (`modactivate_<Datei>`,
  Kap. 7.3) und bleibt immer ganz sichtbar.
- **Status als eigene Spalte** mit drei klar unterscheidbaren Werten: **aktiv**, **inaktiv**
  (Datei vorhanden), **Karteileiche** (Registry-Eintrag ohne Datei — Abgleich Datei ↔ Registry,
  Kap. 7.3, 10.5).
- **Suchen** über Name *und* Dateiname; **Filtern** nach Status, DLC und Author; **Sortieren**
  über jede Spalte. Alles lokal, ohne Nachladen.
- **Keine Detailauskunft hinter „Show more"** — die wichtigen Felder stehen als Spalten da;
  Zusatzdetails höchstens ausklappbar, aber nie anstelle der Kernangaben.
- **Kein endloses Scrollen** als einzige Navigation — die Liste ist durch Suche/Filter/Sortierung
  beherrschbar.
- **Mehrfachauswahl**; Aktivierungs­änderungen werden gesammelt und erst nach Bestätigung
  angewendet.
- **Bei laufendem Server sind die Aktivierungs-Schalter gesperrt** (Auswahl-Checkboxen inaktiv),
  die Tabelle bleibt nur-lesend mit dem Hinweis „zum Ändern erst stoppen" — die Weboberfläche
  lässt Umschalten nur bei gestopptem Spielserver zu (Kap. 7.3).

**G3 Mods bereitstellen.** In der Rangfolge aus F5: **ModHub-Suche** über den lokalen Index
(F5a) mit anschließendem serverseitigem Download samt **Fortschrittsanzeige** (der einzige
AJAX-Endpunkt, Kap. 10.6); alternativ **Datei-Upload** mit Fortschritt und Hinweis auf die
**1,71-GB-Grenze** des Web-Uploads (Kap. 10.10).

**G4 Serversteuerung.** Start / Stopp / Neustart als deutlich getrennte, bestätigungs­pflichtige
Aktionen; der laufende/gestoppte Zustand ist sichtbar. Vor dem Start werden alle
Einstellungs­felder unverändert mitgeführt (Kap. 7.3) — die GUI überschreibt Servername,
Passwörter, Karte oder Slots nicht versehentlich.

**G5 Log-Ansicht.** Fortlaufendes Mitlesen (`tail -f` über HTTP, Kap. 7.4) mit Auswahl der
Logdatei; Fehler im Klartext hervorgehoben (z. B. fehlende Mod-Abhängigkeiten). Diese Ansicht
trägt Q3: Sie ist der Ort, an dem sich Erfolg **nachweisen** lässt.

**G6 Spiel-Einstellungen — eigene Ansicht.** Die Felder des `configuration`-Formulars
(Kap. 7.3), getrennt von Mod-Verwaltung und Steuerung. So soll sie sein:

- **Nach Bedeutung gruppiert**, nicht als eine flache Feldliste:
  - **Identität:** `game_name`, `admin_password`, `game_password`
  - **Spielwelt:** `savegame` (Slot mit Map/Geld-Info), `map_start`, `initialMoney`,
    `initialLoan`, `economicDifficulty`
  - **Netzwerk & Zugang:** `server_port`, `max_player`, `crossplay_allowed`, `mp_language`
  - **Automatik:** `auto_save_interval` (Min.), `stats_interval` (Web-API-Intervall, Sek.),
    `pause_game_if_empty`
- **Passwörter maskiert als Standard, mit „Anzeigen"-Knopf.** `admin_password` und
  `game_password` werden zunächst verdeckt dargestellt; ein Umschalter zeigt sie auf
  ausdrücklichen Wunsch im Klartext (der Admin verwaltet seinen eigenen Server). Anders als die
  Hersteller-Oberfläche, die sie *dauerhaft* offen als Textfelder zeigt (Kap. 7.3), ist die
  Klartext-Anzeige hier eine bewusste Ein-/Ausblendung, keine Voreinstellung. **Unverändert
  hart bleibt Q1:** Passwörter erscheinen **niemals in Logs, Fehlermeldungen oder temporären
  Dateien** — das On-Screen-Anzeigen berührt das nicht.
- **`stats_interval` menschenlesbar** begleiten: Der reine Sekundenwert ist unverständlich
  (z. B. 31536000 = 365 Tage). Die GUI zeigt die Umrechnung und einen kurzen Hinweis, dass
  ein großer Wert den nach außen gereichten Stats-Feed praktisch einfriert (Kap. 7.5).
- **Bearbeiten nur bei gestopptem Server** — bei laufendem Server verschwindet `save_settings`
  (Kap. 7.3); die Ansicht ist dann nur-lesend mit Hinweis „zum Ändern erst stoppen".
- **Speichern schreibt das Formular vollständig zurück** — alle Felder, auch die unveränderten
  (die Bibliothek besorgt den Voll-Formular-Umlauf; Klartext-Passwortfelder laufen dabei durch,
  ohne je protokolliert zu werden — Kap. 7.3).
- **Feldwerte prüfen** vor dem Absenden (erwartete Auswahlwerte/Zahlenbereiche); bei Abweichung
  klar melden statt blind posten (Q4).

**G7 Savegame-Verwaltung (Kann-Ziel, siehe 1.3) — eigene Ansicht, an das Original angelehnt.**
Die Hersteller-Oberfläche bündelt auf `savegames.html` drei Aufgaben untereinander auf einer
Seite (Kap. 7.8); die GUI übernimmt diese Dreiteilung, trennt sie aber in **eigene Reiter**
innerhalb des Menüpunkts „Savegames" statt sie untereinander zu stapeln:

- **Reiter „Manage Savegames"** — Tabelle der belegten Slots: Slot-Name, Map, Geld, Spielzeit,
  Schwierigkeit; je Zeile **Download** (löst den direkten HTTP-Link aus) und **Löschen**
  (bestätigungspflichtig, Q2).
- **Reiter „Upload Savegame"** — Ziel-Slot wählen (belegt oder leer), optional eigener Name,
  ZIP-Datei hochladen, mit Fortschrittsanzeige und Hinweis auf die 1,71-GB-Grenze (analog G3).
- **Reiter „Restore Savegame Backup"** — Dropdown der automatisch angelegten Zeitstempel-Backups
  je Slot; deutlicher Hinweis, dass der Slot dabei überschrieben wird (Q2, bestätigungspflichtig).

Gemeinsame Voraussetzung wie bei G2/G6: Aktionen, die den Serverzustand verändern, respektieren
den gestoppt/laufend-Unterschied, sofern die Weboberfläche das erzwingt (am lebenden Server zu
verifizieren, Kap. 7.8).

> **Nicht im Anfangsumfang: geführter Assistent „Mod-Satz herstellen" (F8).** Der F8-Ablauf
> wird zunächst **von Hand** über Mod-Übersicht (G2), Bereitstellung (G3) und Steuerung (G4)
> erledigt — die Bausteine sind vorhanden. Ein geführter Assistent, der die Schritte selbst
> in der richtigen Reihenfolge abfährt und das Ergebnis nachweist, ist eine spätere Ausbaustufe.

---

## 5. Qualitätsanforderungen

- **Q1 Sicherheit:** Admin-Zugangsdaten im OS-Credential-Store; **niemals** in Protokollen,
  Fehlermeldungen oder temporären Dateien. (Das Admin-Passwort gibt vollen Serverzugriff.)
- **Q2 Vorsicht vor Eingriffen:** Stoppen/Starten und Aktivierungsänderungen sind
  spürbare Eingriffe in einen ggf. bespielten Server — deutliche Bestätigung erforderlich.
- **Q3 Ehrliche Rückmeldung:** kein „Vorgang abgeschlossen" ohne Nachweis; Fehler aus dem
  Log im Klartext zeigen (z. B. fehlende Mod-Abhängigkeiten).
- **Q4 Versionstoleranz:** Erwartete Formularfelder prüfen; bei Abweichung abbrechen mit
  klarer Meldung statt unkontrolliert zu posten.
- **Q5 Plattformen:** Windows, macOS, Linux.

---

## 6. Abnahmekriterien

- **A1** Ein Admin kann sich mit einem Server verbinden und die Mod-Liste sehen.
- **A2** Ein Mod lässt sich aktivieren; die Änderung ist anschließend in der
  Weboberfläche sichtbar.
- **A3** Der Server lässt sich stoppen und starten; der Zustandswechsel ist im Log belegt.
- **A4** Eine Mod-Datei lässt sich hochladen und erscheint danach in der Liste.
- **A5** Der Ablauf F8 läuft vollständig durch und meldet am Ende ein **nachgewiesenes**
  Ergebnis.
- **A6** Bei unerwartetem Aufbau der Weboberfläche bricht das Werkzeug ab, statt zu raten.

---

# Teil B – Verifiziertes technisches Fundament

> Alles in diesem Teil wurde am **echten Server gemessen** (Debian/Docker, Image
> `toetje585/arch-fs25server`, FS25 unter Wine), nicht aus Dokumentation übernommen.
> Beweislage siehe Kap. 9.

## 7. Wie der FS25-Dedicated-Server Mods verwaltet

### 7.1 Die Kette — und warum Dateien zu bearbeiten sinnlos ist

```
SQLite-Datenbank  ──(beim Serverstart erzeugt)──▶  dedicatedServerConfig.xml  ──liest──▶  Spielserver
   (Quelle)                                            (Wegwerf-Export)
        ▲
    Weboberfläche
```

**Gemessener Beleg:** Eine manuell aus der XML gelöschte `<mod>`-Zeile (17:36) war nach dem
Serverstart (17:48:55) wieder vorhanden — die Datei wurde **eine Sekunde vor dem Start neu
erzeugt** (17:48:54).

→ **`dedicatedServerConfig.xml` zu bearbeiten ist wirkungslos.** Community-Anleitungen, die
das empfehlen, meinen meist die andere Datei `dedicatedServer.xml` (Installationsordner,
Servername/Initial-Admin) — nicht diese.

### 7.2 Die Datenbank
`dedicated_server/data/database_v15.dat`, SQLite 3:

```sql
CREATE TABLE gds_mods(id INTEGER PRIMARY KEY AUTOINCREMENT, filename varchar(255), isActive int);
CREATE TABLE gds_gameserver(lastRunState int, autoRestartActive int,
                            hourAutoRestartIntervalBegin int, hourAutoRestartIntervalEnd int,
                            publicModDownloadActive int, loginCount int);
CREATE TABLE gds_stats_feed(feedActive int, accessCodeMD5 varchar(32));
CREATE TABLE gds_users(id, username, name, passwordMD5, passwordSalt, accessFlags);
CREATE TABLE gds_journal(journal_content longtext);
CREATE TABLE gds_tasks(...);
```

**Wichtig:** Wir schreiben hier **nicht** hinein (Nicht-Ziel). Die Kenntnis dient dem
Verständnis — insbesondere, dass `gds_mods.id` vermutlich der `<ID>` in den Formularfeldern
entspricht (Kap. 7.3).

Beobachtung: `gds_mods` enthält **Karteileichen** — Einträge bleiben bestehen, nachdem die
Datei gelöscht wurde. Die Tabelle sagt also **nichts** darüber aus, ob eine Datei existiert.

### 7.3 Die HTTP-Schnittstelle der Weboberfläche
Aus den HTML-Vorlagen in `dedicatedServer.exe` extrahiert:

**Mod-Aktivierung** — zwei getrennte Formulare, beide auf `index.html?lang=<sprache>#mods`:
```html
<form name="ActiveMods"   method="post" action="index.html?lang=en#mods">
  <input type="checkbox" name="moddeactivate_<Dateiname>">  …
  <input type="submit"   name="deactivate_mods">
</form>

<form name="InactiveMods" method="post" action="index.html?lang=en#mods">
  <input type="checkbox" name="modactivate_<Dateiname>">  …
  <input type="submit"   name="activate_mods">
</form>
```

**`InactiveMods` listet nur Mods, deren Datei tatsächlich existiert.** Beleg: Die Datenbank
führte 9 inaktive Einträge, das Formular zeigte nur **3** — die übrigen 6 waren Karteileichen
gelöschter Dateien. Der Server gleicht also selbst gegen den Ordner ab.

**Aktivierungsänderungen sind nur bei gestopptem Spielserver möglich — der Server erzwingt
das selbst** (am laufenden Server gemessen). Läuft der Spielserver, zeigen `ActiveMods` und
`InactiveMods` die Mod-Listen zwar weiter an (Namen sichtbar), aber **ohne Checkboxen und ohne
die Absende-Knöpfe** `activate_mods` / `deactivate_mods`. Bei gestopptem Server sind es 231
bzw. 3 Checkboxen, bei laufendem **0**. Das ist kein Rendern-Unterschied, sondern ein harter
Grund für die F8-Reihenfolge (stoppen → ändern → starten): Die Oberfläche lässt die Umschaltung
im laufenden Betrieb gar nicht zu. Umgekehrt trägt das `configuration`-Formular bei laufendem
Server statt `save_settings` / `start_server` die Knöpfe **`stop_server`** und
**`restart_server`** an gleicher Stelle.

**Einstellungen und Serversteuerung** — ein gemeinsames Formular (verifiziert):
```html
<form name="configuration" method="POST" action="index.html?lang=en">
  game_name · admin_password · game_password · savegame · map_start ·
  initialMoney · initialLoan · economicDifficulty · server_port · max_player ·
  mp_language · auto_save_interval · stats_interval · pause_game_if_empty ·
  crossplay_allowed (checkbox)
  <input type="submit" name="save_settings">
  <input type="submit" name="start_server">
</form>
```
`stop_server` und `restart_server` erscheinen an gleicher Stelle, wenn der Server läuft.

**Formular-Ziele und Anmeldung** (am lebenden Server geprüft)
```html
<form method="POST" action="mods.html?lang=en">   <!-- relativer Seitenname + Query -->
```
Das `#mods` aus der Binärdatei ist nur ein Sprungziel. Die Mod-Formulare posten auf
`index.html?lang=…`.

**Anmeldung** (vollständig verifiziert auf der Login-Seite):
```html
<form name="input" method="POST" action="index.html?lang=en">
  <input type="text"     name="username">
  <input type="password" name="password">
  <input type="submit"   name="login">
</form>
```
Der Server antwortet mit `Set-Cookie`; alle weiteren Anfragen tragen die Sitzung mit.
Abmelden über `index.html?logout=true`.

**Keine versteckten Felder, kein CSRF-Token** — weder auf der Login-Seite noch sonstwo.
Für die Umsetzung bedeutet das: kein Sonderaufwand bei der Anmeldung.

> 🔒 **Sicherheitshinweis (betrifft den Server, nicht unser Werkzeug):** Das Fehlen eines
> CSRF-Tokens bedeutet, dass eine beliebige fremde Webseite im Browser eines angemeldeten
> Admins Anfragen an den Server auslösen könnte — Mods deaktivieren, Server stoppen,
> Einstellungen ändern. Zusammen mit den Klartext-Passwortfeldern im Einstellungs-Formular
> ist ein **öffentlich erreichbares Panel** ein realer Angriffspunkt. Die verbreitete
> Empfehlung, die Weboberfläche nur über VPN oder eingeschränkte IP-Bereiche zugänglich zu
> machen, hat hier ihren Grund. Das Werkzeug sollte darauf hinweisen, wenn es ein
> öffentlich erreichbares Panel vorfindet.

**Die Checkbox-Namen enthalten den Dateinamen, keine Nummer** (am angemeldeten Server geprüft):
```html
<input type="checkbox" name="modactivate_FS25_RealisticLivestockRM.zip">
<input type="checkbox" name="moddeactivate_FS25_DashboardLive_VanillaVehicles.zip">
```
Also `modactivate_<Dateiname mit Endung>` — **stabil und selbsterklärend**, identisch mit der
Spalte `gds_mods.filename`. Keine ID-Zuordnung nötig.

> Hinweis zur Abgrenzung: Der `mod_index` in den Links auf `mods.html` (`mod.html?mod_index=N`)
> ist **flüchtig** — derselbe Mod trug vor einem Serverneustart die 3, danach die 1. Er dient
> nur der Detailansicht und darf **nicht** zwischengespeichert werden. Für die Aktivierung
> ist er ohne Bedeutung.

> ⚠️ **`start_server` steckt im Einstellungs-Formular**, nicht in einem eigenen. Ein nacktes
> `start_server=…` würde alle übrigen Felder leer mitsenden und damit womöglich Servername,
> Passwörter, Karte und Slots überschreiben.
> **Regel:** Vor dem Start das Formular vollständig auslesen und **alle Werte unverändert
> mitschicken**. Dabei laufen `admin_password` und `game_password` als Klartextfelder durch —
> sie dürfen zu keinem Zeitpunkt protokolliert werden.
> Die Mod-Formulare sind davon **nicht** betroffen: Sie enthalten nur Checkboxen und den
> Absende-Knopf. **Aktivieren ist harmlos, Starten ist heikel.**

**Weitere Felder**
| Bereich | Felder |
|---|---|
| Mods | `modUpload` · `startmoddownload` · `cancelmoddownload` · `mod_access_level` |
| Einstellungen | `game_name` · `game_password` · `admin_password` · `server_port` · `map_start` · `savegame` · `crossplay_allowed` · `auto_save_interval` · `stats_interval` · `mp_language` · `save_settings` |
| Dateien | `upload` · `file` · `content_hash` |
| Savegames | `index_upload` · `custom_name` · `backup_restore` · `delete_<Slot>` (Kap. 7.8) |
| Logs | `log_type` · `log_file` · `show_log` · `delete_log` · `log_access_level` |
| Benutzer/Journal | `realname` · `password` · `game_admin` · `is_active` · `journal_content` · `save_journal` · `new_journal_entry` |

**Seiten:** `index.html` (Home, Mods+Steuerung) · `mods.html` (auch `?category=`) ·
`mod.html?mod_index=N` · `savegames.html` · `settings.html` · `users.html` ·
`user.html?user_id=` · `logs.html` · `journal.html` · `profile.html` · `imprint.html`

### 7.4 Live-Log
Inkrementelles Abrufen per POST:
```
log_type=<typ>&log_file=<datei>&offset=<byteposition>&epoch=<kennung>
```
Der Client fragt „gib mir alles ab Byte X" — technisch ein `tail -f` über HTTP. Damit lässt
sich der Erfolg einer Aktion **nachweisen** (`Game server started`) und Fehler im Klartext
zeigen (z. B. `Error: Mod 'FS25_crusher.zip' has missing dependencies`).

### 7.5 Lese-Quellen für den Mod-Bestand
| Quelle | Inhalt | Verfügbarkeit |
|---|---|---|
| `mods.html` | aktive Mods: Name, Version, Author, Dateiname, Größe, Aktiv-Flag; Download-Links | auch bei **gestopptem** Spielserver |
| `mod.html?mod_index=N` | Detailseite je Mod | " |
| `game.xml` bzw. `/feed/dedicated-server-stats.xml?code=…` | Laufzeitstand **mit Hash** | **nur bei laufendem Spielserver** — sonst leere Antwort mit HTTP 200 ⚠️ |

### 7.6 Fallstricke (alle gemessen)
1. **Leere Stats-XML bei gestopptem Server** — HTTP 200, aber keine Mods. Wer das für
   „Server verlangt keine Mods" hält, zieht falsche Schlüsse. **Plausibilität prüfen.**
2. **Zwei Kennungen für denselben Mod:** Config/Formular nutzen den **Dateinamen**
   (`daimlerTruckPack`), Laufzeit und Savegame den **logischen Namen**
   (`pdlc_daimlerTruckPack`).
3. **DLCs** liegen in `pdlc/`, nicht in `mods/`, und werden **nicht** zum Download
   angeboten (231 gelistet, nur 228 herunterladbar).
4. **Doppeltes Escaping** in Anzeigetexten: `author="Kastor [D-S-Agrarservice&amp;#93;"` —
   `]` wurde als `&#93;` kodiert und dann erneut escaped.
5. **Der FS-Hash ist kein md5 der Datei** (siehe ModMatcher-Lastenheft Kap. 10.4).

### 7.7 ModHub-Browsing und -Download (am lebenden Server entschlüsselt)

Der Server kann Mods **selbst aus dem ModHub laden** (F5, bevorzugter Weg). Der gesamte
Mechanismus wurde am angemeldeten Server und an der öffentlichen ModHub-Website vermessen.

**Kategorien statt Suche.** Die Server-Oberfläche bietet **keine Suche** — verifiziert: alle
URL-Such-Parameter (`search=`, `filter=`, `q=`, `name=` …) werden ignoriert, es gibt kein
Suchfeld im HTML. Stattdessen ein **Kategorie-Dropdown**; die `category`-Werte:

| ID | Kategorie | ID | Kategorie |
|---|---|---|---|
| 0 | DLC | 9 | Official Mods |
| 1 | All | 10 | Map (Europe) |
| 3 | Update | 11 | Map (North America) |
| 5 | Latest | 12 | Map (South America) |
| 6 | Best | 13 | Map (other) |
| 7 | Most Downloaded | 14 | Gameplay |
| 8 | Package | | |

Abruf: `GET mods.html?category=<id>&lang=en&page=<p>` — **250 Einträge pro Seite**, Pagination
über `page=`. Nur **angemeldet UND bei gestopptem Server**; sonst (ohne Login *oder* bei
laufendem Server) liefert `?category=` die Liste der installierten Mods — **0
`startmoddownload`-Buttons** (am laufenden Server verifiziert: Katalog und Download sind, wie die
Mod-Umschaltung, nur im gestoppten Zustand verfügbar). Felder je Eintrag: **Name, Version,
Author, Filename, Size, Deps, Issues, Hub, Active**.

**Download auslösen.** Ein Submit-Button je Mod:
```html
<button type="submit" name="startmoddownload" value="366506"
        title="Install FS25_weighingStations18m.zip"> … </button>
```
→ **`startmoddownload=<numerische ModHub-ID>`** per POST auf `mods.html?category=<id>&lang=en`.
Die Kennung ist die **numerische ModHub-ID** (z. B. `366506`), **nicht** der Dateiname.

**Fortschritt.** Der Client pollt (aus `frontend.js`) im **1-Sekunden-Takt**:
```javascript
$.ajax('/mods.html?modhubdownloadprogress=' + modId)  // JSON { downloaded, total }
```
`modId` ist **dieselbe ID** wie bei `startmoddownload`. Abbruch über `cancelmoddownload`.

**Kernbefund — die ID ist website-übergreifend gleich.** Die `startmoddownload`-ID ist
**identisch mit der `mod_id` der öffentlichen ModHub-Website**. Verifiziert an `366506`:
Server-Button → `FS25_weighingStations18m.zip`; `farming-simulator.com/mod.php?mod_id=366506`
→ „Weigh Station Pack", derselbe Dateiname. Das verbindet die öffentliche Suche mit dem
eigenen Server.

**Namenssuche über die ModHub-Website (Weg B, für das Kann-Ziel in 1.3).** Die öffentliche
Website hat eine echte Namenssuche — ein GET-Formular mit dem Feld **`searchMod`**:
```
GET farming-simulator.com/mods.php?title=fs2025&searchMod=<Name>
```
`title=fs2025` wählt das Spiel; die Antwort ist eine **gefilterte Ergebnisliste** mit Name,
Author, Bewertung, Thumbnail und `mod_id` je Treffer (verifiziert: „weighing" → 25 Treffer,
darunter `366506`). Die Detailseite `mod.php?mod_id=<ID>` liefert Version, Dateiname und
Beschreibung. Damit ist die Kette **Namenssuche → Ergebnisliste → Detail → „Install on server"
(`startmoddownload=<mod_id>`) → Fortschritt (`modhubdownloadprogress=<mod_id>`)** vollständig
mit verifizierten Bausteinen gedeckt — **ohne** einen datenaufwendigen lokalen Index.

> ⚠️ **Vorbehalt:** Die ModHub-Website bietet **kein offizielles API** — die Antwort ist
> gerendertes HTML, das geparst werden muss. Ändert GIANTS das Layout, muss der Parser nach
> (dieselbe Versionstoleranz wie bei der Server-Oberfläche, SZ1/Q4).

### 7.8 Savegames verwalten (am lebenden Server verifiziert)

Eigene Seite `savegames.html`, am angemeldeten Testserver (`ccc222`) untersucht. Anders als
zunächst angenommen läuft der Up-/Download **nicht** über FTP/SFTP, sondern über dasselbe
Web-Formular-Muster wie bei Mods (Kap. 7.3). Die Seite gliedert sich in drei Bereiche:

**Übersicht der belegten Slots** — Tabelle mit Slot-Name, Map, Geld, Spielzeit, Schwierigkeit.

**Download — einfacher HTTP-Link, kein Formular:**
```html
<a href="savegame1">Download My game save (1)</a>
<a href="savegame2">Download My game save (2)</a>
```
Pfad ist schlicht `savegame<Slot-Index>` (1–20) relativ zur Seite — GET, kein Login-Overhead
über das bereits bestehende Session-Cookie hinaus.

**Löschen — ebenfalls ein einfacher Link:**
```html
<a href="savegames.html?delete_1=true&lang=en">Delete My game save (1)</a>
```
→ `savegames.html?delete_<Slot-Index>=true`.

**Upload:**
```html
<form method="post" action="savegames.html?lang=en#upload">
  <select name="index_upload"> <!-- Werte 1..20, belegte und leere Slots gemischt --> </select>
  <input type="text" name="custom_name">           <!-- optional -->
  <input type="file" name="file">                   <!-- ZIP-Pflicht, laut Hinweistext -->
  <input type="submit" name="upload" value="Upload">
</form>
```

**Backup wiederherstellen** — eigenständige Funktion, in der bisherigen Doku nicht erfasst: Der
Server legt **automatisch Zeitstempel-Backups je belegtem Slot** an (Format `<Slot>_<Datum
YYYY-MM-DD>_<Zeit HH-MM>`, z. B. `2_2026-07-13_23-56`) und erlaubt, den aktuellen Inhalt des
Slots damit zu überschreiben:
```html
<form method="post" action="savegames.html?lang=en">
  <select name="backup_restore">
    <option value="2_2026-07-13_23-56">Savegame 2 (2026-07-13_23-56) - Map: …</option>
    …
  </select>
  <input type="submit" name="upload" value="Restore">
</form>
```
→ `backup_restore=<Slot>_<Zeitstempel>` per POST; **derselbe** Submit-Name `upload` wie beim
Datei-Upload-Formular, unterschieden nur durch den jeweils mitgesendeten Formularinhalt.

**Einordnung:** Der Mechanismus ist damit dem Mod-Umgang (F5) sehr ähnlich — Web-Formular als
Standardweg, FTP/SFTP nur als Fallback jenseits der 1,71-GB-Grenze (Kap. 7.3) für den seltenen
Fall sehr großer Savegame-Archive.

**Verifiziert: Upload/Delete/Restore funktionieren bei laufendem Server.** Anders als die
Mod-Umschaltung (Kap. 7.3, dort **0** Checkboxen im laufenden Betrieb) sperrt die
Server-Oberfläche Savegame-Upload, -Löschen und -Backup-Restore **nicht** bei laufendem Server —
verhält sich also wie `upload_mod` (Kap. 4.3 PH), nicht wie `set_active`/`delete_mod`.

**Ausnahme: das aktuell geladene Savegame selbst.** Nicht der Serverzustand sperrt hier etwas,
sondern eine **slot-spezifische** Regel — verifiziert an einem laufenden Server mit vier belegten
Slots (Slot 4 = die gerade geladene Karte):
- Die Zeile des aktuell geladenen Slots trägt in der Tabelle **keinen Lösch-Link** — alle
  anderen belegten Slots weiterhin schon, obwohl der Server läuft.
- Im `index_upload`-Dropdown des Upload-Formulars **fehlt genau dieser Slot komplett** — er lässt
  sich nicht als Ziel wählen, man kann das laufende Savegame also nicht überschreiben.

Für die Umsetzung folgt daraus: Die Bibliothek darf die Upload-Zieloptionen **nicht** aus der
Slot-Tabelle synthetisieren (1..20 durchnummeriert) — sie muss die echten `<option>`-Werte des
Dropdowns lesen, sonst böte die GUI einen Slot an, den der Server ablehnt.

---

# Teil C – Anhang

## 8. Offene Punkte

**Geklärt**
- [x] **Anmeldung**: Formularfelder `username` + `password`, Sitzung über `Set-Cookie` (Kap. 7.3).
- [x] **`action`-URL**: relativer Seitenname + Query, z. B. `mods.html?lang=en`, Methode POST.
  `#mods` ist nur ein Sprungziel (Kap. 7.3).
- [x] **Checkbox-Kennung**: **kein** numerisches ID, sondern der **Dateiname**
  (`modactivate_FS25_AutoDrive.zip`) — stabil, entspricht `gds_mods.filename`. Der flüchtige
  `mod_index` betrifft nur die Detailseiten-Links (Kap. 7.3).
- [x] **Formular-Aufbau der Home-Seite** vollständig erfasst: `configuration` (Einstellungen
  **inkl. `start_server`**), `ActiveMods`, `InactiveMods` (Kap. 7.3).
- [x] **`InactiveMods` filtert gegen den Dateibestand** — Karteileichen der Datenbank
  erscheinen nicht (Kap. 7.3).

- [x] **Anmelde-Formular** vollständig: `username` + `password` + `login`, POST auf
  `index.html?lang=…`, **keine versteckten Felder, kein CSRF-Token** (Kap. 7.3).

**Noch offen**
- [x] **Aufbau der ModHub-Kategorieseiten und Download-Mechanismus vollständig entschlüsselt**
  (Kap. 7.7): Kategorie-Dropdown, 250 Einträge/Seite mit Pagination, Download per
  `startmoddownload=<numerische ModHub-ID>`, Fortschritt per `modhubdownloadprogress=<ID>`.
  Die Kennung ist die **numerische ModHub-ID** (identisch mit der `mod_id` der öffentlichen
  Website — an `366506` verifiziert), **nicht** der Dateiname. Die Server-Oberfläche bietet
  **keine Suche**; die Namenssuche fürs Tool läuft über die ModHub-Website (`searchMod=`, Weg B).
- [x] Verhalten von `stop_server` / `restart_server` bei **laufendem** Server bestätigt: Beide
  Knöpfe erscheinen im `configuration`-Formular an gleicher Stelle wie `start_server` (am
  laufenden Server gemessen, Kap. 7.3).
- [x] **Mod-Umschaltung ist bei laufendem Server gesperrt** — `ActiveMods`/`InactiveMods`
  zeigen die Listen ohne Checkboxen und ohne Absende-Knöpfe. Der Server erzwingt damit die
  F8-Reihenfolge selbst (Kap. 7.3). Folge für die GUI: Aktivierungs-Schalter bei laufendem
  Server deaktivieren.
- [x] Gegenprüfung an einem **gemieteten** Server ist **nicht nötig** (Entscheidung): Die
  Weboberfläche ist bei allen Hostern dieselbe GIANTS-Oberfläche (Kap. 9). Der einzige zu
  erwartende Unterschied ist das **Protokoll — HTTPS statt HTTP** — und der macht für die
  Umsetzung **keinen Unterschied**: Für eine HTTP-Bibliothek ist das nur das Schema in der URL,
  alles Weitere (Formulare, Felder, Sitzung) bleibt gleich. Es genügt, dass die Profil-Adresse
  `http` **oder** `https` trägt (Kap. 3 F1). Einziger Randfall: **selbstsignierte oder ungültige
  Zertifikate**, die eine HTTP-Bibliothek standardmäßig ablehnt — das Tool muss sie bewusst
  akzeptieren können.
- [x] **Neue Mod-Dateien werden selbstständig erkannt** und als *inaktiv* registriert —
  und zwar **beim nächsten Seitenaufruf**, nicht erst beim Serverstart (bestätigt vom
  Serverbetreiber). Folgen: Das Werkzeug muss **keine Datenbankzeilen anlegen** und
  **keinen zusätzlichen Neustart** einlegen. Da es die Seite ohnehin frisch liest, bevor es
  absendet, geschieht die Erkennung nebenbei.
- [ ] Technologiewahl und Projektstruktur (Pflichtenheft).
- [x] Produktname festgelegt: **Server Control for Farming Simulator 2025** (im Fließtext kurz
  „Server Control"). Auf Namenskollisionen geprüft (GitHub/Web: keine Treffer); die „for …"-Form
  vermeidet die Marken-Nähe der Abkürzung „FS25".

## 9. Beweislage
| Aussage | Status |
|---|---|
| XML wird beim Serverstart neu erzeugt | **gemessen** (Zeitstempel + Log) |
| SQLite ist die Quelle der Aktivierung | **gemessen** (Schema + Abgleich mit Anzeige) |
| Formularfelder der Weboberfläche | **aus `dedicatedServer.exe` extrahiert** |
| Live-Log mit offset/epoch | **aus `frontend.js` extrahiert** |
| Config wird von der Weboberfläche geschrieben | 1 öffentliche Quelle (redswitches) + eigene Messung |
| Hoster nutzen die GIANTS-Oberfläche | belegt (GPORTAL-, Nitrado-Beschreibungen; Fragnet-URL) |
| **SQLite-Datenbank / `gds_mods`** | **öffentlich nirgends dokumentiert** — keine Suchtreffer |

> Alle Messungen stammen von **einem** Server. Eine Gegenprüfung an einem gemieteten Server
> ist als **nicht nötig** entschieden (Kap. 8): dieselbe GIANTS-Oberfläche, einziger
> Unterschied `https` statt `http` — für die Umsetzung folgenlos, abgesehen vom Randfall
> selbstsignierter/ungültiger Zertifikate, die das Tool bewusst akzeptieren können muss.

---

## 10. Referenzdaten der Messung

Aufbau des vermessenen Servers: Debian auf Hetzner, Docker-Container `arch-fs25server`
(Image `toetje585/arch-fs25server`), FS25 unter Wine. Daher Windows-Pfade in Logs
(`C:/users/nobody/…`) bei Linux-Dateisystem darunter.

### 10.1 Verzeichnisstruktur
```
/opt/fs25/
├─ game/Farming Simulator 2025/      Programmdateien
│  ├─ dedicatedServer.exe            Verwaltungsprozess + Webserver
│  ├─ FarmingSimulator2025.exe       das Spiel
│  └─ web_data/{css,img,js,db,template}   statische Dateien der Weboberfläche
├─ config/FarmingSimulator2025/      Spielprofil („serverProfile")
│  ├─ mods/                          Mod-ZIPs
│  ├─ pdlc/                          DLC-Dateien (getrennt von mods/!)
│  ├─ savegame1…N/, savegameBackup/  Spielstände
│  ├─ modSettings/                   Einstellungen *einzelner Mods* — KEINE Aktivierung
│  ├─ pending_downloads/             Staging für Mod-Downloads
│  └─ dedicated_server/
│     ├─ dedicatedServerConfig.xml   erzeugter Export (nicht bearbeiten)
│     ├─ data/database_v15.dat       SQLite — Quelle der Wahrheit
│     ├─ gameStats.xml, AVD_*.dat    (Zweck ungeklärt)
│     └─ logs/                       server_*.log, webserver_*.log
└─ dlc/, installer/, compose/
```

### 10.2 Prozesse
| Prozess | Rolle |
|---|---|
| `dedicatedServer.exe` | **Verwaltungsprozess**: betreibt die Weboberfläche, startet/stoppt das Spiel, schreibt Datenbank und Config. Läuft **dauerhaft** — auch wenn das Spiel gestoppt ist. |
| `FarmingSimulator2025Game.exe -server` | der eigentliche **Spielserver**. Nur vorhanden, wenn gestartet. |

**Wichtig:** Beide sind getrennt. „Server gestoppt" heißt: Spielprozess weg, Weboberfläche
läuft weiter. Der Verwaltungsprozess hält seinen Zustand **im Arbeitsspeicher** — Änderungen
an Dateien auf der Platte bemerkt er nicht.

### 10.3 Netzwerk-Ports
| Port | Dienst |
|---|---|
| 7999 (bzw. 8080/8443) | Weboberfläche |
| 10823 | Spielserver (TCP+UDP) |
| 5900 / 6080 | VNC / noVNC — **nur in diesem Docker-Image**, nicht Teil von FS |

### 10.4 Logdateien
`dedicated_server/logs/` enthält je Start eine Datei mit Zeitstempel im Namen:
- **`server_<datum>.log`** — Verwaltungsprozess. Protokolliert: Start/Stopp des Spielservers,
  Anmeldungen, **Mod-Uploads und -Löschungen**, Mod-Fehler.
- **`webserver_<datum>.log`** — Zugriffslog im üblichen Format:
  `[22/Jul/2026:17:34:44] <IP> - GET "/mods.html" 200 872928 <User-Agent>`

Beispielzeilen, die für die Erfolgskontrolle taugen:
```
[…] Game server process started
[…] Game server started
[…] Uploaded mod 'FS25_FarmMonitor.zip' to '…/mods/FS25_FarmMonitor.zip'
[…] Mod '…/mods/FS25_annaburgerMilk_eddited.zip' deleted
[…] Error: Mod 'FS25_crusher.zip' (…, V 2.1.0.0) has missing dependencies:
```

### 10.5 Bestandszahlen zum Messzeitpunkt
| Quelle | Wert |
|---|---|
| `mods/` | **231** Dateien, ausnahmslos `.zip` |
| `pdlc/` | **4** `.dlc` (daimlerTruckPack, nexatPack, plainsAndPrairiesPack, extraContentNewHollandCR11) |
| `gds_mods` (Datenbank) | **240** Zeilen = **231 aktiv**, 9 inaktiv |
| `mods.html` | **231** Zeilen, **228** Download-Links (die 3 aktiven DLCs ohne Link) |
| `dedicatedServerConfig.xml` | 231 `<mod>`-Einträge = 228 mit `isDlc="false"` + 3 mit `isDlc="true"` |

**Alle Zahlen sind untereinander konsistent** — die aktive Menge der Datenbank erscheint
identisch in `mods.html` und im XML-Export.

**Die 9 inaktiven Einträge** (zeigen mehrere Eigenheiten auf einmal):
```
FS25_crusher.zip                                     Datei da  — Grund: fehlende Abhängigkeiten (im Log)
FS25_FertilizerProductionPack.zip                    Datei da
FS25_RealisticLivestockRM.zip                        Datei da
extraContentNewHollandCR11.dlc                       DLC vorhanden, aber nicht aktiv
FS25_annaburgerMilk_eddited.zip                      Datei GELÖSCHT → Karteileiche
FS25_System_dryingFermenter.zip                      Datei GELÖSCHT → Karteileiche
FS25_Hirschfeld_LiquidLimeProduction_by_HIP_Marco.zip    Datei GELÖSCHT → Karteileiche
FS25_Hirschfeld_LiquidLimeProduction_by_HIP_Marco_F.zip  Datei GELÖSCHT → Karteileiche
FS25_Fed_Mods_Pack.zip.1                             Artefakt eines Doppel-Uploads (".1")
```
→ Bestätigt: **Die Registry ist kein Spiegel des Ordners.** Ein Werkzeug muss immer gegen
den echten Dateibestand abgleichen.

### 10.6 Weboberfläche: Dateien und Endpunkte
`web_data/` enthält **nur statische Dateien** — die HTML-Seiten erzeugt
`dedicatedServer.exe` selbst (Vorlagen stecken als Zeichenketten in der Binärdatei).

| Datei | Inhalt |
|---|---|
| `js/frontend.js` (167 KB) | Client-Logik; enthält **genau einen** Server-Endpunkt |
| `js/all.js`, `jquery.min.js`, `mobile-menu.js` | Rahmenwerk |
| `db/country_ip.dat` (1,7 MB) | GeoIP-Daten — **kein** Zustandsspeicher |
| `template/` | nur zwei Logo-Bilder |

**Der einzige AJAX-Endpunkt** (alles andere sind klassische Formular-POSTs):
```javascript
$.ajax('/mods.html?modhubdownloadprogress=' + modId)
  // Antwort: JSON { downloaded, total }
```

### 10.7 Datenformate
**Stats-API / `game.xml`** (nur bei laufendem Spielserver befüllt):
```xml
<Server game="Farming Simulator 25" version="1.19.0.0" mapName="…" mapSize="4096"
        mapOverviewFilename="$moddir$FS25_NFMarsch4fach/NFMarsch/pda_map_H.png"
        name="…" dayTime="…">
  <Slots capacity="4" numUsed="0"><Player isUsed="false"/>…</Slots>
  <Mods>
    <Mod name="FS25_AutoDrive" author="AutoDrive Team" version="3.0.1.2"
         hash="129858355f46b0ae43c720e411274326">AutoDrive</Mod>
  </Mods>
  <Farmlands/> <Fields/> <Vehicles/>
</Server>
```
Abruf: `/feed/dedicated-server-stats.xml?code=<TOKEN>` — identisch zur Datei `game.xml`.
Der `$moddir$`-Platzhalter verweist **in ein Mod-ZIP hinein** (Kartenvorschau).

**Savegame** (`savegameN/careerSavegame.xml`), für den Ablauf „Server aus Spielstand":
```xml
<mod modName="FS25_AutoDrive" title="AutoDrive" version="3.0.1.2"
     required="false" fileHash="…"/>
```
Der `fileHash` ist **identisch** mit dem `hash` der Stats-API — dasselbe Token, quellenübergreifend.

**Mods-Seite:** Download-URL je Mod `http://<host>:<port>/mods/<Dateiname>`, öffentlich
ohne Anmeldung (sofern freigeschaltet). Felder je Zeile: Name, Version, Author, Filename,
Size, Issues, Hub, Active.

### 10.8 Server-Einstellungen aus der Datenbank
`gds_gameserver` zum Messzeitpunkt:
```
lastRunState=1   autoRestartActive=0   publicModDownloadActive=1   loginCount=17
gds_stats_feed:  feedActive=1, accessCodeMD5 gesetzt
gds_users:       0 Zeilen  → die Anmeldung nutzt das admin_password aus der Config
```
`publicModDownloadActive=1` ist der Schalter, der die öffentlichen `/mods/…`-Downloads
freigibt. `feedActive=1` schaltet die Stats-API frei.

### 10.9 Stand der öffentlichen Dokumentation
Gezielt geprüft (Seiten geöffnet, nicht nur Suchergebnisse gelesen):

| Quelle | Ergebnis |
|---|---|
| GIANTS-Forumsthread „Won't let me save the dedicatedServer.xml" | **nichts** dazu — es geht um Dateirechte |
| TroubleChute FS25-Guide | **nichts** über Überschreiben oder Datenbank |
| redswitches-Guide | ✅ *„dedicatedServerConfig.xml … Active mods … **This file is automatically updated by the web interface**"* |
| Suche nach `database_v15.dat` / `gds_mods` | **null Treffer** |

**Verwechslungsgefahr, die vieles erklärt:** Es gibt **zwei** XML-Dateien —
`dedicatedServer.xml` (Installationsordner, *zum Bearbeiten gedacht*: Servername,
Initial-Admin) und `dedicatedServerConfig.xml` (Profil, *automatisch verwaltet*). Viele
Anleitungen meinen die erste; wer den Rat auf die zweite überträgt, landet in der Falle.

### 10.10 Weitere Randnotizen
- **Web-Upload endet bei 1,71 GB** — größere Dateien nur per FTP (laut Hersteller-Doku).
- **Mod-Icons**: `<iconFilename>` in der `modDesc.xml`; Dateinamen uneinheitlich, Format
  gemischt **DDS und PNG** (Stichprobe 25 Mods: 12× DDS, 14× PNG).
- **Mods können auch entpackte Ordner sein** — im Multiplayer aber unbrauchbar, da der
  Server ZIPs verteilt und Hashes prüft.
- `gameStats.xml` und `AVD_*.dat` im `dedicated_server/`-Ordner: Zweck nicht untersucht.
