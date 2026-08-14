# Server Control for Farming Simulator 2025

Werkzeug zur **Fernsteuerung eines FS25-Dedicated-Servers über dessen Weboberfläche** —
Mods lesen/aktivieren, Server starten/stoppen, Mods hochladen (Web bis 1,71 GB, darüber
FTP/SFTP), einen lokalen Mod-Ordner mit dem Server abgleichen (nur neuere Versionen
aktualisieren), Mods direkt aus dem ModHub auf dem Server installieren sowie Savegames
verwalten (herunterladen, hochladen, löschen, aus Backup wiederherstellen).

> **Status: funktionsfähig.** Login, Mod-Verwaltung (inkl. ModHub-Download), Server-
> Steuerung, Spieleinstellungen und Savegame-Verwaltung sind umgesetzt.

## Installieren

Fertige Installer (macOS/Linux/Windows) gibt es unter
[Releases](https://github.com/Cypris2010/servercontrol/releases) — kein Rust/Tauri nötig.

## Benutzung

1. **Serverprofil anlegen**: beim ersten Start (oder über den Server-Wähler oben links →
   „Server verwalten…") ein neues Profil mit Adresse (`host:port/index.html`), Benutzername
   und Web-Passwort des Panels anlegen. Das Passwort landet ausschließlich im
   OS-Schlüsselbund, nie in einer Datei der App.
2. **Verbinden**: das Profil im Server-Wähler auswählen. Der Status-Punkt daneben zeigt
   Grün/Grau, ob eine Verbindung besteht; Starten/Stoppen/Neu starten stehen direkt daneben.
3. **Übersicht**: Zustand, Spielversion und Mod-Zahlen auf einen Blick.
4. **Mods**: installierte Mods durchsuchen, einzeln oder per Mehrfachauswahl aktivieren/
   deaktivieren/löschen. Aktivieren/Deaktivieren/Löschen geht nur bei **gestopptem** Server.
5. **Bereitstellen**: neue Mods per Datei- oder Ordnerauswahl hochladen (bis 1,71 GB). Bei der
   Ordnerauswahl lässt sich der komplette lokale Mod-Ordner mit dem Server abgleichen: die App
   vergleicht jede Datei mit dem Serverstand und kann wahlweise alles hochladen/überschreiben
   oder gezielt nur die Mods aktualisieren, von denen lokal eine neuere Version vorliegt.
   Alternativ direkt im offiziellen ModHub nach Mods suchen oder nach Kategorie stöbern (z. B.
   „Update", „Neueste", „Beste") und einen Treffer ohne manuellen Download direkt auf den
   Server installieren — Hochladen wie ModHub-Installation setzen einen gestoppten Server
   voraus.
6. **Spieleinstellungen**: Spielname, Passwörter, Karte, Wirtschaft, Spielerzahl usw. bearbeiten
   (nur bei gestopptem Server als Formular; im laufenden Betrieb read-only als Übersicht).
7. **Savegames**: belegte Slots einsehen, herunterladen oder löschen; ein Savegame in einen
   belegten oder leeren Slot hochladen; ein automatisch angelegtes Zeitstempel-Backup eines
   Slots wiederherstellen. Läuft — anders als der Mod-Upload — bei laufendem **und**
   gestopptem Server; nur das gerade geladene Savegame lässt sich weder löschen noch
   überschreiben.

## Bauen (Entwicklung)

```sh
cargo build            # Workspace (core + cli)
cargo run -p servercontrol-cli

# GUI (Tauri) — braucht die Tauri-CLI (cargo install tauri-cli --version "^2"):
cargo tauri dev        # aus dem Repo-Wurzelverzeichnis
```

## Technik

Rust-Workspace; geplante GUI unter Tauri (Rust-Kern + Web-Oberfläche). Zugangsdaten liegen
ausschließlich im OS-Credential-Store (`keyring`), **nie** im Repo oder in Logs.

## Aufbau

Die **Bibliothek ist der einzige Ort mit Logik**; CLI und (spätere) GUI sind dünne Schichten.

```
servercontrol/
├─ crates/core/    servercontrol-core — die Bibliothek (Sitzung, Mods, Savegames, Steuerung,
│                  Einstellungen, Log, ModHub, Verifikation)
├─ cli/            servercontrol-cli — dünnes Binary auf der Crate
├─ src-tauri/      servercontrol-gui — Tauri-App (dünne Command-Schicht, eigener Workspace)
├─ src/            Web-Oberfläche (Vanilla HTML/CSS/JS, kein Bundler)
├─ docs/           Lasten- und Pflichtenheft (verbindliche Spezifikation)
├─ mockups/        interaktiver GUI-Mockup (visuelle Referenz) + Icon-Lizenz
├─ tools/          Prüf-/Aufnahmeskripte (Fixtures, Formular-Abgleich)
└─ .github/        CI (Build/Test-Matrix Windows/macOS/Linux)
```

Die **GUI** ist ein eigener Workspace (`src-tauri/`), bewusst aus dem Kern-Workspace
ausgeschlossen — so bauen `cargo build --workspace` und die CI plattformarm nur core+cli. Der
GUI-Mockup unter `mockups/` dient als visuelle Vorlage fürs Frontend.

## Dokumentation

- [Lastenheft](docs/Lastenheft-ServerControl.md) — das *Was* (Anforderungen)
- [Pflichtenheft](docs/Pflichtenheft-ServerControl.md) — das *Wie* (Technik, API, GUI-Spec)
- [GUI-Mockup](mockups/servercontrol-mockup.html) — interaktiv, mit Beispieldaten

## Lizenzen Dritter

- Icons im GUI-Mockup (`mockups/`, nicht in der eigentlichen App): [Tabler Icons](https://tabler.io/icons)
  (MIT) — siehe [`mockups/ICON-CREDITS.md`](mockups/ICON-CREDITS.md).
