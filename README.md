# Server Control for Farming Simulator 2025

Werkzeug zur **Fernsteuerung eines FS25-Dedicated-Servers über dessen Weboberfläche** —
Mods lesen/aktivieren, Server starten/stoppen, Mods hochladen (Web bis 1,71 GB, darüber
FTP/SFTP) und ModHub-Downloads anstoßen.

> **Status: Gerüst / Entwurf 0.1.** Konzept steht (Lasten-/Pflichtenheft, GUI-Mockup),
> die Umsetzung beginnt. Die Bibliotheks-Operationen sind noch Stubs (`todo!()`).

## Aufbau

Die **Bibliothek ist der einzige Ort mit Logik**; CLI und (spätere) GUI sind dünne Schichten.

```
servercontrol/
├─ crates/core/    servercontrol-core — die Bibliothek (Sitzung, Mods, Steuerung,
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

## Bauen

```sh
cargo build            # Workspace (core + cli)
cargo run -p servercontrol-cli

# GUI (Tauri) — braucht die Tauri-CLI (cargo install tauri-cli --version "^2"):
cargo tauri dev        # aus dem Repo-Wurzelverzeichnis
```

## Technik

Rust-Workspace; geplante GUI unter Tauri (Rust-Kern + Web-Oberfläche). Zugangsdaten liegen
ausschließlich im OS-Credential-Store (`keyring`), **nie** im Repo oder in Logs.

## Lizenzen Dritter

- Icons: [Tabler Icons](https://tabler.io/icons) (MIT) — siehe
  [`mockups/ICON-CREDITS.md`](mockups/ICON-CREDITS.md).
