//! Bundled helpers that can be refreshed at runtime. Today that is yt-dlp, which needs updates
//! as video sites change: "Update YouTube importer" fetches the latest release, verifies it
//! against the release's SHA2-256SUMS, and stores it in app data. The bundled copy stays as fallback.
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::brand::BRAND;

pub fn updates_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(&BRAND.identifier)
        .join("bin")
}

const YTDLP_RELEASE: &str = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";
const YTDLP_ASSET: &str = if cfg!(target_os = "macos") {
    "yt-dlp_macos"
} else if cfg!(windows) {
    "yt-dlp.exe"
} else {
    "yt-dlp_linux"
};

/// Finds `asset` in a SHA2-256SUMS file ("<hash>  <name>" per line).
pub fn sum_for(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, name) = l.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == asset).then(|| hash.trim().to_lowercase())
    })
}

/// Returns the installed version.
pub async fn update_ytdlp() -> Result<String> {
    let client = reqwest::Client::builder().user_agent(&BRAND.slug).build()?;
    let rel: Value = client
        .get(YTDLP_RELEASE)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let tag = rel["tag_name"]
        .as_str()
        .context("no release tag")?
        .to_string();
    let url_of = |name: &str| {
        rel["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["name"] == name)
            .and_then(|a| a["browser_download_url"].as_str().map(String::from))
    };
    let sums = client
        .get(url_of("SHA2-256SUMS").context("release has no checksums")?)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let expected =
        sum_for(&sums, YTDLP_ASSET).context("checksum list has no entry for this platform")?;
    let bin = client
        .get(url_of(YTDLP_ASSET).context("release has no binary for this platform")?)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    ensure!(
        format!("{:x}", Sha256::digest(&bin)) == expected,
        "the download failed its checksum, keeping the current version"
    );
    std::fs::create_dir_all(updates_dir())?;
    let tmp = updates_dir().join("yt-dlp.part");
    std::fs::write(&tmp, &bin)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, updates_dir().join("yt-dlp"))?;
    Ok(tag)
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_sha256sums() {
        let sums = "aa11  yt-dlp\nbb22  yt-dlp_macos\ncc33 *yt-dlp.exe\n";
        assert_eq!(
            super::sum_for(sums, "yt-dlp_macos").as_deref(),
            Some("bb22")
        );
        assert_eq!(super::sum_for(sums, "yt-dlp.exe").as_deref(), Some("cc33"));
        assert!(super::sum_for(sums, "nope").is_none());
    }
}
