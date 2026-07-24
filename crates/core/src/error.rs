use thiserror::Error;

/// Fehlerfälle der Bibliothek (Pflichtenheft Kap. 4.5).
///
/// Die GUI/CLI bilden diese auf verständliche Meldungen ab (Kap. 7.11).
#[derive(Debug, Error)]
pub enum Error {
    /// Panel/Host nicht erreichbar.
    #[error("Server nicht erreichbar")]
    Unreachable,

    /// Web-Login abgelehnt (Zugangsdaten falsch).
    #[error("Anmeldung abgelehnt – Zugangsdaten prüfen")]
    AuthFailed,

    /// Profil vorhanden, aber kein Passwort im Credential-Store (extern gelöscht /
    /// Profil auf anderen Rechner kopiert).
    #[error("Kein Passwort im Credential-Store – bitte neu eingeben")]
    CredentialMissing,

    /// Erwartetes Formularfeld fehlt → nicht posten (Versionstoleranz, Q4/SZ1).
    #[error("Serverformat nicht erkannt: {0}")]
    FormMismatch(String),

    /// Umschalten/Löschen/ModHub-Install nur bei gestopptem Server (Kap. 7.3 LH).
    #[error("Aktion nur bei gestopptem Server möglich")]
    ServerRunning,

    /// Datei > 1,71 GB, aber kein FTP/SFTP im Profil hinterlegt (MZ4).
    #[error("Für große Dateien FTP/SFTP im Profil einrichten")]
    NoFileAccess,

    /// FTP/SFTP-Login abgelehnt (eigener Zugang, ≠ Web-Login).
    #[error("FTP/SFTP-Zugangsdaten prüfen")]
    FileAuthFailed,

    /// FTP/SFTP-Host/Port nicht erreichbar.
    #[error("FTP/SFTP-Server nicht erreichbar")]
    FileUnreachable,

    /// Abbruch mitten in einer Übertragung (HTTP oder FTP/SFTP).
    #[error("Übertragung fehlgeschlagen")]
    TransferFailed,

    /// Aktion abgesetzt, aber im Log/Zustand nicht bestätigt (Q3).
    #[error("Aktion abgesetzt, aber nicht bestätigt")]
    NotProven,

    /// Vom Nutzer abgebrochen.
    #[error("Abgebrochen")]
    Cancelled,

    /// Allgemeiner Netzwerkfehler.
    #[error("Netzwerkfehler: {0}")]
    Network(String),

    /// Auswertungsfehler (HTML/JSON unerwartet aufgebaut).
    #[error("Auswertungsfehler: {0}")]
    Parse(String),
}
