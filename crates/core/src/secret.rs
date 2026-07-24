use std::fmt;

/// Passwort-Wrapper, der seinen Inhalt **niemals** ausdruckt (Q1, Pflichtenheft 4.2).
///
/// `Debug`/`Display` zeigen nur `***`, damit ein Passwort nicht versehentlich in Log,
/// Fehlermeldung oder Trace landet. Der Klartext ist nur über [`Secret::expose`] erreichbar
/// (bewusst benannt), z. B. im Moment des POST.
///
/// Platzhalter: in der Umsetzung durch `secrecy::SecretString` (+ `zeroize`) ersetzen, damit
/// der Speicher beim Freigeben zusätzlich genullt wird.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Expliziter Zugriff auf den Klartext — nur intern verwenden.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

impl From<String> for Secret {
    fn from(s: String) -> Self {
        Self(s)
    }
}
