//! Livecheck: Login + Status gegen einen echten FS25-Server.
//!
//! Das Passwort liegt im OS-Credential-Store (Schlüssel `livetest/web`) und wird von dort
//! gelesen — es steht nie im Code und wird nie ausgegeben. `SC_PASSWORD` ist **optional**:
//!   - **gesetzt:** das Passwort wird (neu) im Store hinterlegt, dann Login (Erst-Einrichtung),
//!   - **nicht gesetzt:** es wird das **bereits hinterlegte** Passwort verwendet.
//!
//! So lässt sich der Livecheck nach einmaliger Einrichtung ohne Passwort wiederholen:
//!   SC_URL="http://<host>:7999/index.html" SC_USER=admin \
//!     cargo run -p servercontrol-core --example login
//!
//! Erfolg = „Anmeldung erfolgreich". Falsches Passwort = „Anmeldung abgelehnt".

use servercontrol_core::{store_password, ModStatus, OpCtx, Secret, ServerControl, ServerProfile};
use url::Url;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    let base_url =
        std::env::var("SC_URL").expect("SC_URL setzen, z. B. http://host:7999/index.html");
    let username = std::env::var("SC_USER").unwrap_or_else(|_| "admin".to_string());

    // Feste ID (kein `new_v4`), damit ein wiederholter Livecheck denselben Credential-Store-
    // Eintrag findet.
    let id = Uuid::nil();
    let credential_key = ServerProfile::web_credential_key(id);

    // SC_PASSWORD optional: gesetzt → einmal im Store hinterlegen; sonst den vorhandenen
    // Eintrag nutzen (Code liest ihn ohnehin nur über den Schlüssel).
    if let Ok(password) = std::env::var("SC_PASSWORD") {
        store_password(&credential_key, Secret::new(password))
            .expect("Passwort speichern fehlgeschlagen");
    }

    let profile = ServerProfile {
        id,
        name: "Livetest".to_string(),
        base_url: Url::parse(&base_url).expect("SC_URL ist keine gültige URL"),
        username,
        credential_key,
        accept_invalid_cert: false,
        file_access: None,
    };

    match ServerControl::connect(&profile, &OpCtx).await {
        Ok(sc) => {
            println!("Anmeldung erfolgreich — Sitzung steht.");
            match sc.state().await {
                Ok(state) => println!("Status: {state:?}"),
                Err(e) => println!("Status konnte nicht gelesen werden: {e}"),
            }
            match sc.list_mods().await {
                Ok(mods) => {
                    let active = mods
                        .iter()
                        .filter(|m| m.status == ModStatus::Active)
                        .count();
                    let dlc = mods.iter().filter(|m| m.is_dlc).count();
                    println!(
                        "Mods: {} gesamt ({active} aktiv, {} inaktiv, {dlc} DLC)",
                        mods.len(),
                        mods.len() - active,
                    );
                }
                Err(e) => println!("Mods konnten nicht gelesen werden: {e}"),
            }
            match sc.read_settings().await {
                Ok(s) => println!("Einstellungen: {s:?}"), // Secret-Debug zeigt Passwörter als ***
                Err(e) => println!("Einstellungen nicht lesbar: {e}"),
            }
            match sc.read_settings_options().await {
                Ok(o) => println!(
                    "Optionen: {} Maps, {} Savegames, {} Sprachen | Maps: {:?}",
                    o.maps.len(),
                    o.savegames.len(),
                    o.mp_language.len(),
                    o.maps.iter().map(|m| &m.label).collect::<Vec<_>>(),
                ),
                Err(e) => println!("Optionen nicht lesbar: {e}"),
            }
            match sc.logout().await {
                Ok(()) => println!("Abgemeldet."),
                Err(e) => println!("Abmelden meldete: {e}"),
            }
        }
        Err(e) => println!("Login nicht erfolgreich: {e}"),
    }
}
