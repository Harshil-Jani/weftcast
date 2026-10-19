//! Android phones over USB (or Wi-Fi debugging): adb to find them, scrcpy to record the screen.
//!
//! scrcpy (Apache-2.0) is bundled. adb is Google's (Android SDK License, which does not allow
//! redistribution), so it is not bundled: the user agrees to Google's terms in the app and adb is
//! downloaded from Google once, checked against a pinned SHA-256, into app data.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{media, sidecars};

/// Google's platform-tools release this app was tested with (pinned; bump URL and hash together).
pub const PLATFORM_TOOLS_URL: &str =
    "https://dl.google.com/android/repository/platform-tools_r37.0.1-darwin.zip";
pub const PLATFORM_TOOLS_SHA256: &str =
    "ee39ad5967e95c2a07f04dbcbde96b1a0c916ba376096db5d2f498b7727a5d1d";
pub const PLATFORM_TOOLS_TERMS: &str = "https://developer.android.com/studio/terms";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AndroidDevice {
    pub serial: String,
    pub model: String,
    /// "device" when ready; "unauthorized" until the phone accepts USB debugging.
    pub state: String,
}

pub fn adb_path() -> Option<PathBuf> {
    let p = sidecars::updates_dir().join("platform-tools").join("adb");
    if p.is_file() {
        return Some(p);
    }
    // A developer's own adb (Android Studio, Homebrew) works too.
    let t = media::tool("adb");
    t.is_file().then_some(t)
}

/// The scrcpy server that runs on the phone: bundled next to the app's resources.
pub fn scrcpy_server() -> PathBuf {
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("../Resources/scrcpy-server")))
        .filter(|p| p.exists());
    bundled.unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries/scrcpy-server"))
}

/// Parses `adb devices -l`.
pub fn parse_devices(out: &str) -> Vec<AndroidDevice> {
    out.lines()
        .skip_while(|l| !l.starts_with("List of devices"))
        .skip(1)
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let serial = parts.next()?.to_string();
            let state = parts.next()?.to_string();
            let model = parts
                .find_map(|p| p.strip_prefix("model:"))
                .map(|m| m.replace('_', " "))
                .unwrap_or_else(|| "Android phone".into());
            Some(AndroidDevice {
                serial,
                model,
                state,
            })
        })
        .collect()
}

pub async fn devices() -> Vec<AndroidDevice> {
    let Some(adb) = adb_path() else {
        return vec![];
    };
    let out = tokio::process::Command::new(adb)
        .args(["devices", "-l"])
        .output()
        .await;
    out.map(|o| parse_devices(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// Downloads Google's platform-tools (after the user agreed to Google's terms in the app),
/// verifies the pinned hash and keeps only adb.
pub async fn install_tools() -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent(&crate::brand::BRAND.slug)
        .build()?;
    let bytes = client
        .get(PLATFORM_TOOLS_URL)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let got = hex(&Sha256::digest(&bytes));
    ensure!(
        got == PLATFORM_TOOLS_SHA256,
        "The download did not match the expected file. Nothing was installed."
    );
    let dir = sidecars::updates_dir();
    std::fs::create_dir_all(&dir)?;
    let zip = dir.join("platform-tools.zip");
    std::fs::write(&zip, &bytes)?;
    let status = tokio::process::Command::new("/usr/bin/unzip")
        .args([
            "-oq",
            &zip.to_string_lossy(),
            "platform-tools/adb",
            "platform-tools/NOTICE.txt",
            "-d",
            &dir.to_string_lossy(),
        ])
        .status()
        .await?;
    let _ = std::fs::remove_file(&zip);
    ensure!(status.success(), "Could not unpack Android tools");
    let adb = adb_path().context("adb is missing after install")?;
    let v = tokio::process::Command::new(&adb)
        .arg("version")
        .output()
        .await?;
    Ok(String::from_utf8_lossy(&v.stdout)
        .lines()
        .next()
        .unwrap_or("adb")
        .to_string())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// scrcpy arguments to record a phone's screen to `out` without a window or sound.
pub fn record_args(serial: &str, out: &Path) -> Vec<String> {
    vec![
        "--serial".into(),
        serial.into(),
        "--no-playback".into(),
        "--no-window".into(),
        "--no-audio".into(),
        "--no-control".into(),
        "--max-fps=30".into(),
        format!("--record={}", out.to_string_lossy()),
    ]
}

// ---------- Wi-Fi (Android 11+ wireless debugging) ----------

/// A phone address from the Wireless debugging screen: an IPv4 address and a port.
pub fn wifi_addr(addr: &str) -> anyhow::Result<String> {
    let a = addr.trim();
    let (ip, port) = a.rsplit_once(':').ok_or_else(|| {
        anyhow::anyhow!("Enter the address and port shown on the phone, like 192.168.1.20:37099.")
    })?;
    let ok_ip = ip.parse::<std::net::Ipv4Addr>().is_ok();
    let ok_port = port.parse::<u16>().is_ok_and(|p| p > 0);
    anyhow::ensure!(
        ok_ip && ok_port,
        "Enter the address and port shown on the phone, like 192.168.1.20:37099."
    );
    Ok(a.to_string())
}

/// `adb pair <addr> <code>`: the code and port from "Pair device with pairing code".
pub fn pair_args(addr: &str, code: &str) -> anyhow::Result<Vec<String>> {
    let addr = wifi_addr(addr)?;
    let code: String = code.chars().filter(char::is_ascii_digit).collect();
    anyhow::ensure!(
        code.len() == 6,
        "Enter the 6-digit pairing code shown on the phone."
    );
    Ok(vec!["pair".into(), addr, code])
}

/// `adb connect <addr>`: the address and port at the top of the Wireless debugging screen.
pub fn connect_args(addr: &str) -> anyhow::Result<Vec<String>> {
    Ok(vec!["connect".into(), wifi_addr(addr)?])
}

/// adb prints success on stdout and many failures on stdout too, with exit code 0.
pub fn wifi_result(out: &str) -> anyhow::Result<()> {
    let o = out.to_lowercase();
    if o.contains("successfully paired")
        || o.contains("connected to") && !o.contains("failed") && !o.contains("cannot")
    {
        return Ok(());
    }
    if o.contains("already connected") {
        return Ok(());
    }
    anyhow::bail!(if o.contains("refused")
        || o.contains("no route")
        || o.contains("timed out")
        || o.contains("cannot connect")
    {
        "The phone did not answer. Check it is on the same Wi-Fi and that Wireless debugging is still on."
    } else if o.contains("wrong password") || o.contains("failed") {
        "Pairing did not work. Check the code and port, which change each time the phone shows them."
    } else {
        "The phone did not answer. Try again."
    })
}

/// Runs one adb Wi-Fi step (pair or connect).
pub async fn wifi(args: Vec<String>) -> anyhow::Result<()> {
    let adb =
        adb_path().ok_or_else(|| anyhow::anyhow!("Get Android Tools first, then try again."))?;
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        tokio::process::Command::new(adb).args(&args).output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("The phone did not answer. Try again."))??;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    wifi_result(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_adb_devices() {
        let out = "* daemon started successfully\nList of devices attached\n\
                   R5CT1234ABC            device usb:1-1 product:o1sxxx model:SM_G991B device:o1s transport_id:1\n\
                   192.168.1.30:5555      unauthorized transport_id:2\n\n";
        let d = parse_devices(out);
        assert_eq!(d.len(), 2);
        assert_eq!(
            d[0],
            AndroidDevice {
                serial: "R5CT1234ABC".into(),
                model: "SM G991B".into(),
                state: "device".into()
            }
        );
        assert_eq!(d[1].state, "unauthorized");
        assert_eq!(d[1].model, "Android phone");
    }

    #[test]
    fn scrcpy_records_headless() {
        let a = record_args("R5CT", Path::new("/tmp/p.mp4"));
        assert!(
            a.contains(&"--no-window".to_string())
                && a.contains(&"--record=/tmp/p.mp4".to_string())
        );
    }

    #[test]
    fn wifi_pair_and_connect_commands() {
        assert_eq!(
            pair_args(" 192.168.1.20:37099 ", "123 456").unwrap(),
            vec!["pair", "192.168.1.20:37099", "123456"]
        );
        assert_eq!(
            connect_args("192.168.1.20:41235").unwrap(),
            vec!["connect", "192.168.1.20:41235"]
        );
        assert!(
            pair_args("192.168.1.20", "123456").is_err(),
            "needs the port"
        );
        assert!(
            pair_args("phone.local:1234", "123456").is_err(),
            "an IPv4 address, as the phone shows"
        );
        assert!(pair_args("192.168.1.20:0", "123456").is_err());
        assert!(
            pair_args("192.168.1.20:37099", "12345").is_err(),
            "six digits"
        );
        assert!(connect_args("192.168.1.20:99999").is_err());
        assert!(wifi_result("Successfully paired to 192.168.1.20:37099 [guid=adb-X]").is_ok());
        assert!(wifi_result("connected to 192.168.1.20:41235").is_ok());
        assert!(wifi_result("already connected to 192.168.1.20:41235").is_ok());
        assert!(wifi_result("Failed: Wrong password or connection was dropped.").is_err());
        assert!(
            wifi_result("failed to connect to '192.168.1.20:41235': Connection refused").is_err()
        );
    }
}
