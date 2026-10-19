//! iPhone screen over Wi-Fi: the iPhone mirrors to this Mac with AirPlay (Control Center, Screen
//! Mirroring, pick this Mac), macOS shows it in a window, and that window is recorded as the
//! phone track. No cable and no app on the phone.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// Asks the window server for windows and displays (no permission needed for owners and bounds).
const LIST_JS: &str = r#"ObjC.import('CoreGraphics'); ObjC.import('AppKit');
var w = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly | $.kCGWindowListExcludeDesktopElements, 0))) || [];
var scr = $.NSScreen.screens, out = [];
for (var i = 0; i < scr.count; i++) { var o = scr.objectAtIndex(i), f = o.frame; out.push({x: f.origin.x, y: f.origin.y, w: f.size.width, h: f.size.height, scale: o.backingScaleFactor}); }
JSON.stringify({windows: w.map(function (x) { return {owner: x.kCGWindowOwnerName || "", name: x.kCGWindowName || null, layer: x.kCGWindowLayer || 0, b: x.kCGWindowBounds}; }), screens: out});"#;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Bounds {
    #[serde(rename = "X")]
    pub x: f64,
    #[serde(rename = "Y")]
    pub y: f64,
    #[serde(rename = "Width")]
    pub w: f64,
    #[serde(rename = "Height")]
    pub h: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Win {
    pub owner: String,
    pub name: Option<String>,
    pub layer: i64,
    pub b: Bounds,
}

/// A display, in AppKit points (origin bottom-left of the main display).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Screen {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub scale: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Snapshot {
    pub windows: Vec<Win>,
    pub screens: Vec<Screen>,
}

pub fn parse(json: &str) -> Result<Snapshot> {
    serde_json::from_str(json).context("could not read the window list")
}

pub async fn snapshot() -> Result<Snapshot> {
    let out = Command::new("/usr/bin/osascript")
        .args(["-l", "JavaScript", "-e", LIST_JS])
        .output()
        .await
        .context("could not list windows")?;
    parse(&String::from_utf8_lossy(&out.stdout))
}

/// The AirPlay mirroring window, if an iPhone is mirroring now: the largest normal window owned by
/// the system's AirPlay or Screen Mirroring process.
pub fn find_mirror(s: &Snapshot) -> Option<&Win> {
    s.windows
        .iter()
        .filter(|w| {
            let o = w.owner.to_lowercase();
            (o.contains("airplay") || o.contains("mirroring"))
                && w.layer == 0
                && w.b.w >= 160.0
                && w.b.h >= 160.0
        })
        .max_by(|a, b| (a.b.w * a.b.h).total_cmp(&(b.b.w * b.b.h)))
}

/// Which display holds the window (by its centre), and that display's index in the system's
/// order, which is the order ffmpeg lists "Capture screen N".
pub fn display_of(s: &Snapshot, w: &Win) -> Option<(usize, Screen)> {
    let main_h = s.screens.first()?.h;
    let (cx, cy) = (w.b.x + w.b.w / 2.0, w.b.y + w.b.h / 2.0);
    s.screens.iter().enumerate().find_map(|(i, d)| {
        // Window bounds are top-left based; AppKit frames are bottom-left based.
        let top = main_h - (d.y + d.h);
        (cx >= d.x && cx < d.x + d.w && cy >= top && cy < top + d.h).then(|| (i, d.clone()))
    })
}

/// Height of a window title bar, in points: cut off so only the phone screen is recorded.
pub const TITLE_BAR_PT: f64 = 28.0;

/// The ffmpeg crop for the window inside its display, in pixels (even sizes, kept on screen).
pub fn crop(w: &Win, d: &Screen, main_h: f64) -> String {
    let top = main_h - (d.y + d.h);
    let chrome = if w.name.is_some() { TITLE_BAR_PT } else { 0.0 };
    let x = ((w.b.x - d.x).max(0.0) * d.scale).round() as i64;
    let y = ((w.b.y - top + chrome).max(0.0) * d.scale).round() as i64;
    let max_w = (d.w * d.scale) as i64 - x;
    let max_h = (d.h * d.scale) as i64 - y;
    let cw = ((w.b.w * d.scale) as i64).min(max_w) & !1;
    let ch = (((w.b.h - chrome) * d.scale) as i64).min(max_h) & !1;
    format!("crop={cw}:{ch}:{x}:{y}")
}

/// Whether AirPlay Receiver is on: it listens on port 7000 while on. None when it cannot tell.
pub async fn receiver_on() -> Option<bool> {
    let out = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-iTCP:7000", "-sTCP:LISTEN"])
        .output()
        .await
        .ok()?;
    if String::from_utf8_lossy(&out.stdout).contains("ControlCe") {
        return Some(true);
    }
    let off = Command::new("/usr/bin/defaults")
        .args(["read", "com.apple.controlcenter", "AirplayRecieverEnabled"])
        .output()
        .await
        .ok()?;
    match String::from_utf8_lossy(&off.stdout).trim() {
        "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{"windows":[
        {"owner":"Window Server","name":"Cursor","layer":2147483630,"b":{"X":614,"Y":561,"Width":28,"Height":40}},
        {"owner":"Control Centre","name":"Item-0","layer":25,"b":{"X":783,"Y":0,"Width":34,"Height":30}},
        {"owner":"Safari","name":"Docs","layer":0,"b":{"X":0,"Y":30,"Width":1352,"Height":848}},
        {"owner":"AirPlayUIAgent","name":null,"layer":0,"b":{"X":10,"Y":40,"Width":120,"Height":120}},
        {"owner":"AirPlayUIAgent","name":"iPhone","layer":0,"b":{"X":900,"Y":100,"Width":390,"Height":872}}
    ],"screens":[{"x":0,"y":0,"w":1352,"h":878,"scale":2},{"x":1352,"y":-200,"w":1920,"h":1080,"scale":1}]}"#;

    #[test]
    fn finds_the_mirroring_window() {
        let s = parse(FIXTURE).unwrap();
        let w = find_mirror(&s).unwrap();
        assert_eq!(
            (w.owner.as_str(), w.b.w),
            ("AirPlayUIAgent", 390.0),
            "the big one, not the tiny one"
        );
        let none = parse(r#"{"windows":[{"owner":"Safari","name":"x","layer":0,"b":{"X":0,"Y":0,"Width":800,"Height":600}}],"screens":[]}"#).unwrap();
        assert!(find_mirror(&none).is_none(), "no iPhone mirroring");
    }

    #[test]
    fn crops_to_the_window_without_its_title_bar() {
        let s = parse(FIXTURE).unwrap();
        let w = find_mirror(&s).unwrap();
        let (i, d) = display_of(&s, w).unwrap();
        assert_eq!(i, 0);
        // 2x display: x 900pt -> 1800px; y 100+28pt -> 256px; height (872-28)pt -> 1688px, clipped to
        // the display's 1756px - 256px = 1500px.
        assert_eq!(crop(w, &d, 878.0), "crop=780:1500:1800:256");
        // A window on the second display (1x, to the right of the main one).
        let w2 = Win {
            owner: "AirPlayUIAgent".into(),
            name: None,
            layer: 0,
            b: Bounds {
                x: 2000.0,
                y: 100.0,
                w: 401.0,
                h: 801.0,
            },
        };
        let (i2, d2) = display_of(&s, &w2).unwrap();
        assert_eq!(i2, 1);
        // Second display's top in window coordinates: 878 - (-200 + 1080) = -2; so y = 102px; odd sizes rounded down.
        assert_eq!(crop(&w2, &d2, 878.0), "crop=400:800:648:102");
    }
}
