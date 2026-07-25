//! Wegwerf-Livetest für Schritt 1: prüft den echten Login gegen einen FS25-Server.
//!
//! Das Passwort wird **nur** aus der Umgebungsvariablen `SC_PASSWORD` gelesen — es steht
//! nirgends im Code und wird nie ausgegeben. Ablauf:
//!   1. Passwort in den OS-Credential-Store legen (wie es die App später auch tut),
//!   2. damit `connect` ausführen (echter GET+POST-Login),
//!   3. `logout`.
//!
//! Ausführen (in deinem Terminal):
//!   SC_URL="http://<host>:7999/index.html" SC_USER=admin SC_PASSWORD='...' \
//!     cargo run -p servercontrol-core --example login
//!
//! Erfolg = „Anmeldung erfolgreich". Falsches Passwort = „Anmeldung abgelehnt".

use servercontrol_core::{store_password, OpCtx, Secret, ServerControl, ServerProfile};
use url::Url;

#[tokio::main]
async fn main() {
    let base_url =
        std::env::var("SC_URL").expect("SC_URL setzen, z. B. http://host:7999/index.html");
    let username = std::env::var("SC_USER").unwrap_or_else(|_| "admin".to_string());
    let password = std::env::var("SC_PASSWORD").expect("SC_PASSWORD in der Shell setzen");

    let credential_key = "livetest/web";

    // Passwort in den Credential-Store (danach kennt der Code nur noch den Schlüssel).
    store_password(credential_key, Secret::new(password))
        .expect("Passwort speichern fehlgeschlagen");

    let profile = ServerProfile {
        name: "Livetest".to_string(),
        base_url: Url::parse(&base_url).expect("SC_URL ist keine gültige URL"),
        username,
        credential_key: credential_key.to_string(),
        accept_invalid_cert: false,
        file_access: None,
    };

    match ServerControl::connect(&profile, &OpCtx).await {
        Ok(sc) => {
            println!("Anmeldung erfolgreich — Sitzung steht.");
            match sc.logout().await {
                Ok(()) => println!("Abgemeldet."),
                Err(e) => println!("Abmelden meldete: {e}"),
            }
        }
        Err(e) => println!("Login nicht erfolgreich: {e}"),
    }
}
