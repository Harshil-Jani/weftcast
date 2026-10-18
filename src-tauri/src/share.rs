//! Public links through a Cloudflare quick tunnel (`cloudflared tunnel --url`).
//! No account needed. The link works only while the app is running.
use std::process::Stdio;

use anyhow::{Context, Result, bail};
use regex::Regex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::media;

#[derive(Default)]
pub struct Tunnel {
    inner: tokio::sync::Mutex<Option<(Child, String)>>,
}

pub fn installed() -> bool {
    media::available("cloudflared")
}

impl Tunnel {
    pub async fn url(&self) -> Option<String> {
        self.inner.lock().await.as_ref().map(|(_, u)| u.clone())
    }

    /// Starts the tunnel to the local server (or returns the running one's URL).
    pub async fn start(&self, port: u16) -> Result<String> {
        let mut g = self.inner.lock().await;
        if let Some((_, url)) = g.as_ref() {
            return Ok(url.clone());
        }
        if !installed() {
            bail!("The public-link helper is missing from this build. Reinstall the app.");
        }
        let mut child = Command::new(media::tool("cloudflared"))
            .args([
                "tunnel",
                "--no-autoupdate",
                "--url",
                &format!("http://127.0.0.1:{port}"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("could not start cloudflared")?;
        let mut lines = BufReader::new(child.stderr.take().context("stderr")?).lines();
        let re = Regex::new(r"https://[a-z0-9-]+\.trycloudflare\.com").unwrap();
        let url = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while let Ok(Some(l)) = lines.next_line().await {
                if let Some(m) = re.find(&l) {
                    return Some(m.as_str().to_string());
                }
            }
            None
        })
        .await
        .ok()
        .flatten()
        .context(
            "Cloudflare did not hand out a link within 30 seconds. Check your internet connection.",
        )?;
        // Keep draining stderr so cloudflared never blocks on a full pipe.
        tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
        // The hostname takes a few seconds to resolve; only hand out a link that already works.
        let probe = format!("{url}/v/ready-check");
        for _ in 0..20 {
            if reqwest::get(&probe)
                .await
                .is_ok_and(|r| r.status().as_u16() == 404)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        }
        *g = Some((child, url.clone()));
        Ok(url)
    }

    pub async fn stop(&self) {
        if let Some((mut child, _)) = self.inner.lock().await.take() {
            let _ = child.kill().await;
        }
    }
}
