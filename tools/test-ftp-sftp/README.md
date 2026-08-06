# tools/test-ftp-sftp/

Lokale Docker-Testserver für FTP **und** SFTP (MZ4, Pflichtenheft Kap. 5). Beide Protokolle
sind fester Bestandteil (kein Kann), daher gibt es hier zwei Container statt einem — die
`FileAccess`/`FileProtocol`-Umsetzung (`crates/core/src/profile.rs`) soll gegen beide getestet
werden, bevor sie gegen den echten Zielserver läuft.

**Nur für lokale Tests.** Testzugangsdaten sind absichtlich hart codiert (kein echtes Ziel,
kein Schutzbedarf) — nichts hier ist ein Ersatz für den OS-Credential-Store (Q1), den die
Bibliothek für echte Profile weiter zwingend nutzt.

## Start

```sh
docker compose up -d
```

| Protokoll | Host/Port | User | Passwort | mods-Pfad |
|---|---|---|---|---|
| SFTP | `localhost:2222` | `testuser` | `testpass` | `/mods` |
| FTP | `localhost:2121` (passiv 21100–21110) | `testuser` | `testpass` | `/` |

Damit ergibt sich z. B. für SFTP:

```rust
FileAccess {
    protocol: FileProtocol::Sftp,
    host: "localhost".into(),
    port: 2222,
    username: "testuser".into(),
    credential_key: /* eigener Eintrag im Credential-Store, Passwort "testpass" */,
    mods_path: "/mods".into(),
}
```

## Public-Key-Auth testen (SFTP)

Die Anforderung aus der MZ4-Planung ist, dass Key-Auth als Option unterstützt wird, nicht nur
Passwort. Testschlüssel erzeugen:

```sh
./gen-key.sh
docker compose restart sftp
```

Verbindet dann z. B.:

```sh
sftp -i keys/id_test -P 2222 testuser@localhost
```

## Reset

```sh
docker compose down -v
rm -rf data/mods/* keys/id_test keys/id_test.pub
```

## Grenzen dieses Setups

- `delfer/alpine-ftp-server` prüft **nicht** case-sensitiv wie das FS25-Webpanel (siehe
  Pflichtenheft 6.2) — das ist ein reines FTP/SFTP-Dateisystem, keine HTTP-Session-Eigenheit.
- Passive FTP-Ports sind hier auf `127.0.0.1` fest verdrahtet; für Tests aus einer VM/einem
  anderen Host `ADDRESS` in `docker-compose.yml` anpassen.
- **Wichtig:** `delfer/alpine-ftp-server` nutzt **nicht** die bei anderen Alpine-FTP-Images
  üblichen `FTP_USER`/`FTP_PASS`/`PASV_ADDRESS`-Variablen, sondern `USERS` (Format
  `name|password|folder|uid|gid`) sowie `ADDRESS`/`MIN_PORT`/`MAX_PORT`. Siehe
  `/bin/start_vsftpd.sh` im Image. Am Produktivserver live verifiziert (2026-07-30).

## Produktiv-Deployment (Hetzner-Server, echtes Serverprofil)

Zusätzlich zur lokalen Sandbox oben läuft eine Instanz auf dem echten Hetzner-Server
(`178.105.224.182`, `/opt/fs25/compose/ftp-sftp/`), gemountet auf das komplette
`/opt/fs25/config/FarmingSimulator2025` (nicht nur `mods/`) — zum Testen der MZ4-Umsetzung
gegen echte Savegames/Configs, bevor sie an den eigentlichen Zielserver geht.

**Sicherheitsentscheidungen dort (bewusst abweichend von der lokalen Sandbox):**
- Ports sind **öffentlich** gebunden (`0.0.0.0`, da der Server keine Firewall hat) —
  deshalb **kein** Testpasswort, sondern generierte Zugangsdaten.
- SFTP läuft **ausschließlich mit Public-Key-Auth** (Passwortfeld im `atmoz/sftp`-Command
  ist leer: `testuser::1000:1000`) — kein Passwort-Login möglich.
- FTP braucht zwangsläufig ein Passwort (vsftpd unterstützt kein Key-Auth). Es wurde
  **auf dem Server selbst** per `openssl rand` generiert und liegt nur in
  `/opt/fs25/compose/ftp-sftp/.env` (chmod 600) — nie in einer lokalen Datei oder einem
  Chat-Verlauf.
- Der private SFTP-Testkey liegt lokal unter `prod-keys/id_prod_sftp` (gitignored,
  **nie committen**).

Zugangsdaten für diese Produktiv-Instanz bei Bedarf direkt auf dem Server nachschauen/rotieren:

```sh
ssh -i ~/.ssh/dedi_server_key root@178.105.224.182 \
  "cat /opt/fs25/compose/ftp-sftp/.env"   # FTP-Passwort
```

Stack verwalten:

```sh
ssh -i ~/.ssh/dedi_server_key root@178.105.224.182 \
  "cd /opt/fs25/compose/ftp-sftp && docker compose ps"
```
