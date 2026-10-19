//! Serves the library's watch pages and media without the app: `cargo run --example serve`.
//!
//! For headless tests of Connect Phone it prints `PORT <n>`, and:
//! - `PAIR_ROLE=camera|angle` issues a pairing and prints `PAIR <token>` and `CODE <6 digits>`;
//! - `PAIR_TUNNEL=1` opens the temporary public link and prints `TUNNEL <url>`;
//! - lines on stdin: `issue` (a new pairing), `end` (end it, as when a recording ends), `lan` (start
//!   the offline page on the local network and print `LAN <url>`);
//! - remote button presses are printed as `REMOTE <action>`.
use std::io::Write;

use app_lib::pairing::{PAIR, REMOTE, Role};
use tokio::io::AsyncBufReadExt;

fn issue(role: Role, base: &str) {
    let token = PAIR.lock().unwrap().issue(role, std::time::Instant::now());
    let code = PAIR
        .lock()
        .unwrap()
        .current()
        .map(|p| p.code.clone())
        .unwrap_or_default();
    // The real QR encoder, for UI fixtures.
    if let Ok(out) = std::env::var("QR_OUT") {
        let _ = std::fs::write(
            out,
            app_lib::pairing::qr_svg(&format!("{base}/phone#t={token}")).unwrap_or_default(),
        );
    }
    println!("PAIR {token}");
    println!("CODE {code}");
    let _ = std::io::stdout().flush();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = app_lib::server::start().await?;
    println!("PORT {port}");
    for v in app_lib::library::list() {
        println!("http://127.0.0.1:{port}/v/{}  {}", v.share_id, v.title);
    }
    let role = match std::env::var("PAIR_ROLE").as_deref() {
        Ok("angle") => Some(Role::Angle),
        Ok(_) => Some(Role::Camera),
        Err(_) => None,
    };
    let tunnel = app_lib::share::Tunnel::default();
    let mut base = format!("http://127.0.0.1:{port}");
    if std::env::var("PAIR_TUNNEL").is_ok() {
        base = tunnel.start(port).await?;
        println!("TUNNEL {base}");
    }
    if let Some(r) = role {
        issue(r, &base);
    }
    let mut rx = REMOTE.subscribe();
    tokio::spawn(async move {
        while let Ok(a) = rx.recv().await {
            println!("REMOTE {a}");
            let _ = std::io::stdout().flush();
        }
    });
    let _ = std::io::stdout().flush();
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            line = lines.next_line() => match line?.as_deref().map(str::trim) {
                Some("issue") => issue(role.unwrap_or(Role::Camera), &base),
                Some("end") => app_lib::pairing::end_pairing(),
                Some("lan") => {
                    let (ip, p) = app_lib::pairing::start_lan().await?;
                    println!("LAN https://{ip}:{p}");
                }
                Some(_) => {}
                None => break,
            },
        }
        let _ = std::io::stdout().flush();
    }
    tunnel.stop().await;
    Ok(())
}
