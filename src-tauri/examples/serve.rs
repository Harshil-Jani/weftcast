//! Serves the library's watch pages and media without the app: `cargo run --example serve`.
//! For headless tests of the phone page: `PAIR_ROLE=camera|angle` issues a pairing token and prints
//! `PAIR <token>`; remote button presses are printed as `REMOTE <action>`.
use std::io::Write;

use app_lib::pairing::{PAIR, REMOTE, Role};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = app_lib::server::start().await?;
    for v in app_lib::library::list() {
        println!("http://127.0.0.1:{port}/v/{}  {}", v.share_id, v.title);
    }
    if let Ok(role) = std::env::var("PAIR_ROLE") {
        let role = if role == "angle" {
            Role::Angle
        } else {
            Role::Camera
        };
        let token = PAIR.lock().unwrap().issue(role, std::time::Instant::now());
        // The real QR encoder, for UI fixtures.
        if let (Ok(url), Ok(out)) = (std::env::var("QR_URL"), std::env::var("QR_OUT")) {
            std::fs::write(out, app_lib::pairing::qr_svg(&format!("{url}#t={token}"))?)?;
        }
        println!("PAIR {token}");
    }
    let mut rx = REMOTE.subscribe();
    tokio::spawn(async move {
        while let Ok(a) = rx.recv().await {
            println!("REMOTE {a}");
            let _ = std::io::stdout().flush();
        }
    });
    let _ = std::io::stdout().flush();
    tokio::signal::ctrl_c().await?;
    Ok(())
}
