//! "Send to my iPhone": hands the pairing link to AirDrop, so the phone gets a notification and
//! opens it with one tap. macOS only; the share sheet itself is the system's.

use anyhow::{Result, bail};

/// The link AirDrop sends: only this app's pairing page, over HTTPS, of a sane length.
pub fn link(url: &str) -> Result<String> {
    let url = url.trim();
    if !url.starts_with("https://") || !url.contains("/phone") {
        bail!("Only the pairing link can be sent.");
    }
    if url.len() > 2048 || url.chars().any(char::is_whitespace) {
        bail!("That link is not valid.");
    }
    Ok(url.to_string())
}

/// Opens the AirDrop picker for the link. Runs on the main thread, as AppKit requires.
#[cfg(target_os = "macos")]
pub fn send(app: &tauri::AppHandle, url: &str) -> Result<()> {
    use objc2::rc::Retained;
    use objc2_app_kit::{NSSharingService, NSSharingServiceNameSendViaAirDrop};
    use objc2_foundation::{NSArray, NSString, NSURL};

    let url = link(url)?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<()>>();
    app.run_on_main_thread(move || {
        let out = (|| {
            // SAFETY: AppKit calls on the main thread with valid, retained objects.
            unsafe {
                let Some(svc) =
                    NSSharingService::sharingServiceNamed(NSSharingServiceNameSendViaAirDrop)
                else {
                    bail!("AirDrop is not available on this Mac.");
                };
                let Some(u) = NSURL::URLWithString(&NSString::from_str(&url)) else {
                    bail!("That link is not valid.");
                };
                let obj: Retained<objc2::runtime::AnyObject> =
                    Retained::into_super(Retained::into_super(u));
                let items = NSArray::from_retained_slice(&[obj]);
                if !svc.canPerformWithItems(Some(&items)) {
                    bail!("Turn on AirDrop and Bluetooth on this Mac, then try again.");
                }
                svc.performWithItems(&items);
            }
            Ok(())
        })();
        let _ = tx.send(out);
    })?;
    rx.recv_timeout(std::time::Duration::from_secs(5))
        .unwrap_or_else(|_| bail!("AirDrop did not open. Try the QR code instead."))
}

#[cfg(not(target_os = "macos"))]
pub fn send(_app: &tauri::AppHandle, _url: &str) -> Result<()> {
    bail!("AirDrop is only on macOS. Scan the QR code instead.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_pairing_link_is_sent() {
        let ok = "https://abc-def.trycloudflare.com/phone#t=0123abcd";
        assert_eq!(link(ok).unwrap(), ok);
        assert_eq!(
            link("  https://192.168.0.26:8443/phone#t=x \n").unwrap(),
            "https://192.168.0.26:8443/phone#t=x"
        );
        assert!(
            link("http://abc.trycloudflare.com/phone").is_err(),
            "never plain http"
        );
        assert!(
            link("https://example.com/").is_err(),
            "not the pairing page"
        );
        assert!(link("file:///etc/passwd").is_err());
        assert!(link("https://a.b/phone x").is_err(), "no spaces");
        assert!(
            link(&format!("https://a.b/phone#{}", "x".repeat(3000))).is_err(),
            "not huge"
        );
    }
}
