#!/usr/bin/env sh
# Erzeugt ein Test-Schlüsselpaar für die Public-Key-Auth gegen den SFTP-Testcontainer.
# Öffentlicher Schlüssel landet in keys/ (wird beim nächsten `docker compose up` gemountet),
# privater Schlüssel bleibt lokal (siehe .gitignore) und ist NICHT für echte Server gedacht.
set -eu
cd "$(dirname "$0")"
ssh-keygen -t ed25519 -f keys/id_test -N "" -C "servercontrol-test-key"
echo "Public Key: keys/id_test.pub (wird in den sftp-Container gemountet)"
echo "Private Key zum Testen: keys/id_test"
echo "Container neu starten, damit der Key gezogen wird: docker compose restart sftp"
