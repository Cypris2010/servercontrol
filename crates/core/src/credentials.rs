//! Zugriff auf den OS-Credential-Store (Q1, Pflichtenheft 4.2 / Kap. 6).
//!
//! Passwörter liegen **ausschließlich** hier — macOS Keychain, Windows Credential Manager,
//! Linux Secret Service. Das Profil trägt nur den `credential_key`, nie das Passwort selbst.

use keyring::Entry;

use crate::error::Error;
use crate::secret::Secret;
use crate::Result;

/// Dienstname, unter dem alle Einträge im Store gruppiert sind.
const SERVICE: &str = "servercontrol";

/// Passwort zu einem `credential_key` laden.
///
/// Fehlt der Eintrag (extern gelöscht / Profil auf anderen Rechner kopiert), meldet die
/// Funktion gezielt [`Error::CredentialMissing`] — abgegrenzt von `AuthFailed` (falsches
/// Passwort), damit die Oberfläche „bitte neu eingeben" statt „Daten prüfen" zeigen kann.
pub(crate) fn load(credential_key: &str) -> Result<Secret> {
    let entry = Entry::new(SERVICE, credential_key).map_err(map_keyring)?;
    match entry.get_password() {
        Ok(password) => Ok(Secret::new(password)),
        Err(keyring::Error::NoEntry) => Err(Error::CredentialMissing),
        Err(e) => Err(map_keyring(e)),
    }
}

/// Passwort zu einem `credential_key` speichern/aktualisieren.
pub(crate) fn store(credential_key: &str, password: &Secret) -> Result<()> {
    let entry = Entry::new(SERVICE, credential_key).map_err(map_keyring)?;
    entry.set_password(password.expose()).map_err(map_keyring)
}

/// Passwort zu einem `credential_key` entfernen (Kap. 8.4: beim Löschen eines Profils
/// werden beide zugehörigen Einträge mitentfernt). Fehlender Eintrag ist kein Fehler.
pub(crate) fn delete(credential_key: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, credential_key).map_err(map_keyring)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(map_keyring(e)),
    }
}

fn map_keyring(e: keyring::Error) -> Error {
    // Store-Fehler tragen keinen Passwort-Inhalt; die Meldung ist unbedenklich.
    Error::Network(format!("Credential-Store: {e}"))
}
