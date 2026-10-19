//! Pairing a phone over Wi-Fi: one-time token in a QR code, the phone page, WebRTC signalling,
//! the remote control, and track upload.
//!
//! Two sides, two sets of routes:
//! - phone routes (`/phone`, `/api/phone/*`) need the pairing token. They are served on the LAN over
//!   HTTPS (a self-signed certificate made on first use; browsers only allow the camera on a secure
//!   page), and on the local server, which the optional public tunnel forwards to.
//! - app routes (`/api/pair/*`) are for this app's own windows only: they refuse the public tunnel
//!   and any browser origin other than the app's, so a web page cannot read or answer the offer.
//!
//! The video itself goes phone to app directly over WebRTC (on the LAN, host candidates only);
//! these routes only carry the offer and answer, button presses and status.
use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use once_cell::sync::{Lazy, OnceCell};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::brand::BRAND;
use crate::library;
use crate::phone::{self, Track, TrackKind};

/// A paired phone's job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Face cam: feeds the camera bubble and is recorded as its own track.
    Camera,
    /// Second angle: recorded as an extra track (hands, a whiteboard, a product).
    Angle,
}

/// An unused token expires after this; a connected phone stays paired until a recording ends.
pub const UNUSED_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug)]
pub struct Pairing {
    token: String,
    pub role: Role,
    issued: Instant,
    connected: bool,
    pub device: Option<String>,
    offer: Option<String>,
    answer: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum AuthError {
    NoPairing,
    WrongToken,
    Expired,
}

#[derive(Default)]
pub struct Store {
    cur: Option<Pairing>,
}

/// Compares without stopping at the first difference, so timing reveals nothing about the token.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

impl Store {
    /// A new token replaces any previous pairing.
    pub fn issue(&mut self, role: Role, now: Instant) -> String {
        let token = uuid::Uuid::new_v4().simple().to_string();
        self.cur = Some(Pairing {
            token: token.clone(),
            role,
            issued: now,
            connected: false,
            device: None,
            offer: None,
            answer: None,
        });
        token
    }

    pub fn auth(&mut self, token: &str, now: Instant) -> Result<&mut Pairing, AuthError> {
        let p = self.cur.as_mut().ok_or(AuthError::NoPairing)?;
        if !same(&p.token, token) {
            return Err(AuthError::WrongToken);
        }
        if !p.connected && now.duration_since(p.issued) > UNUSED_TTL {
            self.cur = None;
            return Err(AuthError::Expired);
        }
        Ok(self.cur.as_mut().expect("checked above"))
    }

    /// Called when a recording ends (saved or discarded): the token stops working.
    pub fn end(&mut self) {
        self.cur = None;
    }

    pub fn current(&self) -> Option<&Pairing> {
        self.cur.as_ref()
    }
}

pub static PAIR: Lazy<Mutex<Store>> = Lazy::new(Default::default);
/// Remote button presses from the phone ("start", "pause", "resume", "stop", "discard").
pub static REMOTE: Lazy<tokio::sync::broadcast::Sender<String>> =
    Lazy::new(|| tokio::sync::broadcast::channel(16).0);
/// Recording status for the phone's remote tab, posted by the app's controls window.
static STATUS: Lazy<Mutex<Value>> = Lazy::new(|| {
    Mutex::new(json!({"recording": false, "paused": false, "elapsed": 0, "level": 0}))
});

pub fn end_pairing() {
    PAIR.lock().unwrap().end();
}

// ---------- phone routes ----------

const PHONE_HTML: &str = include_str!("phone.html");

#[derive(Deserialize)]
struct TokenQ {
    token: String,
}

fn denied(e: AuthError) -> Response {
    let msg = match e {
        AuthError::NoPairing | AuthError::Expired => {
            "This pairing has ended. Scan the new code in the app."
        }
        AuthError::WrongToken => "This code is not valid. Scan the code shown in the app.",
    };
    (StatusCode::UNAUTHORIZED, Json(json!({"error": msg}))).into_response()
}

async fn phone_page() -> Html<String> {
    let brand = json!({"name": BRAND.name, "accent": BRAND.accent}).to_string();
    Html(
        PHONE_HTML
            .replace("__BRAND__", &brand)
            .replace("__BRAND_NAME__", &BRAND.name),
    )
}

#[derive(Deserialize)]
struct Hello {
    token: String,
    device: String,
}

async fn hello(Json(h): Json<Hello>) -> Response {
    let mut s = PAIR.lock().unwrap();
    match s.auth(&h.token, Instant::now()) {
        Ok(p) => {
            p.connected = true;
            p.device = Some(h.device.chars().take(60).collect());
            Json(json!({"role": p.role, "brand": BRAND.name})).into_response()
        }
        Err(e) => denied(e),
    }
}

#[derive(Deserialize)]
struct Sdp {
    token: String,
    sdp: String,
}

async fn offer(Json(o): Json<Sdp>) -> Response {
    let mut s = PAIR.lock().unwrap();
    match s.auth(&o.token, Instant::now()) {
        Ok(p) => {
            p.offer = Some(o.sdp);
            p.answer = None;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => denied(e),
    }
}

async fn answer_for_phone(Query(q): Query<TokenQ>) -> Response {
    let mut s = PAIR.lock().unwrap();
    match s.auth(&q.token, Instant::now()) {
        Ok(p) => Json(json!({"sdp": p.answer})).into_response(),
        Err(e) => denied(e),
    }
}

#[derive(Deserialize)]
struct Press {
    token: String,
    action: String,
}

async fn remote(Json(r): Json<Press>) -> Response {
    let ok = PAIR
        .lock()
        .unwrap()
        .auth(&r.token, Instant::now())
        .map(|_| ());
    if let Err(e) = ok {
        return denied(e);
    }
    if !["start", "pause", "resume", "stop", "discard"].contains(&r.action.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "unknown button"})),
        )
            .into_response();
    }
    let _ = REMOTE.send(r.action);
    StatusCode::NO_CONTENT.into_response()
}

async fn status_for_phone(Query(q): Query<TokenQ>) -> Response {
    let ok = PAIR
        .lock()
        .unwrap()
        .auth(&q.token, Instant::now())
        .map(|_| ());
    match ok {
        Ok(()) => Json(STATUS.lock().unwrap().clone()).into_response(),
        Err(e) => denied(e),
    }
}

pub fn phone_routes() -> Router {
    Router::new()
        .route("/phone", get(phone_page))
        .route("/brand.png", get(brand_png))
        .route("/api/phone/hello", post(hello))
        .route("/api/phone/offer", post(offer))
        .route("/api/phone/answer", get(answer_for_phone))
        .route("/api/phone/remote", post(remote))
        .route("/api/phone/status", get(status_for_phone))
}

/// The app icon: the brand mark on the phone page and the watch page.
async fn brand_png() -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        include_bytes!("../icons/64x64.png").as_slice(),
    )
        .into_response()
}

// ---------- app routes ----------

/// The app's own web origins (bundled app on macOS, and the dev server).
const APP_ORIGINS: [&str; 3] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "http://localhost:1420",
];

fn from_app(h: &HeaderMap) -> bool {
    let public = h.contains_key("cf-connecting-ip");
    let origin_ok = match h.get(header::ORIGIN).and_then(|o| o.to_str().ok()) {
        None => true,
        Some(o) => APP_ORIGINS.contains(&o),
    };
    !public && origin_ok
}

async fn app_state(h: HeaderMap) -> Response {
    if !from_app(&h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let s = PAIR.lock().unwrap();
    Json(match s.current() {
        None => json!({"paired": false}),
        Some(p) => json!({"paired": true, "role": p.role, "connected": p.connected, "device": p.device, "offer": p.offer, "answered": p.answer.is_some()}),
    })
    .into_response()
}

#[derive(Deserialize)]
struct AppAnswer {
    sdp: String,
}

async fn app_answer(h: HeaderMap, Json(a): Json<AppAnswer>) -> Response {
    if !from_app(&h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match PAIR.lock().unwrap().cur.as_mut() {
        Some(p) => {
            p.answer = Some(a.sdp);
            StatusCode::NO_CONTENT.into_response()
        }
        None => StatusCode::CONFLICT.into_response(),
    }
}

async fn app_status(h: HeaderMap, Json(v): Json<Value>) -> Response {
    if !from_app(&h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    *STATUS.lock().unwrap() = json!({
        "recording": v["recording"].as_bool().unwrap_or(false),
        "paused": v["paused"].as_bool().unwrap_or(false),
        "elapsed": v["elapsed"].as_f64().unwrap_or(0.0),
        "level": v["level"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0),
    });
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct TrackQ {
    video: String,
    kind: TrackKind,
    /// Wall-clock ms when this track started recording.
    start: u64,
    #[serde(default)]
    device: String,
    #[serde(default)]
    ext: String,
}

/// Offset of a track against the screen, in seconds, from two wall-clock start times.
pub fn offset_secs(track_start_ms: u64, screen_start_ms: u64) -> f64 {
    (track_start_ms as f64 - screen_start_ms as f64) / 1000.0
}

/// A phone camera track recorded in the app window arrives here once the recording is saved.
async fn app_track(h: HeaderMap, Query(q): Query<TrackQ>, body: Bytes) -> Response {
    if !from_app(&h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match save_track(&q, &body).await {
        Ok(v) => Json(json!({"tracks": v.tracks})).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn save_track(q: &TrackQ, body: &[u8]) -> Result<library::Video> {
    anyhow::ensure!(
        q.kind != TrackKind::PhoneScreen,
        "phone screen tracks are recorded by the app"
    );
    anyhow::ensure!(!body.is_empty(), "the track is empty");
    let mut v = library::load(&q.video).context("unknown video")?;
    let ext = if q.ext == "mp4" { "mp4" } else { "webm" };
    let raw = library::dir(&v.id).join(format!(
        "tracks/raw-{}.{ext}",
        q.kind
            .file()
            .trim_start_matches("tracks/")
            .trim_end_matches(".mp4")
    ));
    tokio::fs::create_dir_all(raw.parent().expect("tracks folder")).await?;
    tokio::fs::write(&raw, body).await?;
    let offset = if v.rec_started_ms > 0 {
        offset_secs(q.start, v.rec_started_ms)
    } else {
        0.0
    };
    let duration = if v.rec_duration > 0.0 {
        v.rec_duration
    } else {
        v.duration
    };
    phone::normalize(&raw, &phone::track_path(&v.id, q.kind), offset, duration).await?;
    v.tracks.retain(|t| t.kind != q.kind);
    v.tracks.push(Track {
        kind: q.kind,
        device: q.device.chars().take(60).collect(),
        offset,
    });
    library::save(&v)?;
    phone::render(&v.id).await?;
    Ok(v)
}

pub fn app_routes() -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(APP_ORIGINS.map(HeaderValue::from_static)))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE]);
    Router::new()
        .route("/api/pair/state", get(app_state))
        .route("/api/pair/answer", post(app_answer))
        .route("/api/pair/status", post(app_status))
        // A few minutes of 720p from the phone.
        .route(
            "/api/pair/track",
            post(app_track).layer(DefaultBodyLimit::max(4 << 30)),
        )
        .layer(cors)
}

// ---------- LAN listener, QR ----------

/// This Mac's address on the local network (no packet is sent: UDP connect only picks a route).
pub fn lan_ip() -> Option<std::net::IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:80").ok()?;
    s.local_addr()
        .ok()
        .map(|a| a.ip())
        .filter(|ip| !ip.is_loopback())
}

fn cert_dir() -> PathBuf {
    crate::sidecars::updates_dir().join("lan-cert")
}

/// A self-signed certificate for the LAN page, made once and reused (phones remember the exception).
fn cert(ip: std::net::IpAddr) -> Result<(Vec<u8>, Vec<u8>)> {
    let d = cert_dir();
    let (c, k) = (d.join("cert.pem"), d.join("key.pem"));
    let ip_file = d.join("ip.txt");
    if c.exists()
        && k.exists()
        && std::fs::read_to_string(&ip_file).ok().as_deref() == Some(&ip.to_string())
    {
        return Ok((std::fs::read(&c)?, std::fs::read(&k)?));
    }
    std::fs::create_dir_all(&d)?;
    let names = vec![
        ip.to_string(),
        "localhost".into(),
        format!("{}.local", BRAND.slug),
    ];
    let rcgen::CertifiedKey { cert, key_pair } = rcgen::generate_simple_self_signed(names)?;
    std::fs::write(&c, cert.pem())?;
    std::fs::write(&k, key_pair.serialize_pem())?;
    std::fs::write(&ip_file, ip.to_string())?;
    Ok((
        cert.pem().into_bytes(),
        key_pair.serialize_pem().into_bytes(),
    ))
}

static LAN: OnceCell<(std::net::IpAddr, u16)> = OnceCell::new();

/// Starts the HTTPS phone page on the local network (once), only when the user asks to pair.
pub async fn start_lan() -> Result<(std::net::IpAddr, u16)> {
    if let Some(a) = LAN.get() {
        return Ok(*a);
    }
    let ip = lan_ip()
        .context("This Mac is not on a network. Connect to Wi-Fi, or use a public link.")?;
    let (c, k) = cert(ip)?;
    let tls = axum_server::tls_rustls::RustlsConfig::from_pem(c, k).await?;
    let listener = std::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0)))?;
    let port = listener.local_addr()?.port();
    let server = axum_server::from_tcp_rustls(listener, tls);
    tokio::spawn(async move {
        let _ = server.serve(phone_routes().into_make_service()).await;
    });
    let _ = LAN.set((ip, port));
    Ok((ip, port))
}

/// The QR code as an SVG drawn with the current text colour.
pub fn qr_svg(url: &str) -> Result<String> {
    let code = qrcode::QrCode::new(url.as_bytes())?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(232, 232)
        .dark_color(qrcode::render::svg::Color("#000000"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_authenticate_and_expire() {
        let mut s = Store::default();
        let t0 = Instant::now();
        assert_eq!(s.auth("x", t0).unwrap_err(), AuthError::NoPairing);
        let tok = s.issue(Role::Camera, t0);
        assert_eq!(tok.len(), 32);
        assert_eq!(
            s.auth("wrong-token-wrong-token-wrong-tok", t0).unwrap_err(),
            AuthError::WrongToken
        );
        assert_eq!(s.auth(&tok[..31], t0).unwrap_err(), AuthError::WrongToken);
        assert!(s.auth(&tok, t0 + Duration::from_secs(60)).is_ok());
        // Unused for longer than the TTL: gone.
        assert_eq!(
            s.auth(&tok, t0 + UNUSED_TTL + Duration::from_secs(1))
                .unwrap_err(),
            AuthError::Expired
        );
        assert_eq!(s.auth(&tok, t0).unwrap_err(), AuthError::NoPairing);
        // Once connected it lasts until the recording ends, however long that takes.
        let tok = s.issue(Role::Angle, t0);
        s.auth(&tok, t0).unwrap().connected = true;
        assert!(s.auth(&tok, t0 + UNUSED_TTL * 4).is_ok());
        s.end();
        assert_eq!(s.auth(&tok, t0).unwrap_err(), AuthError::NoPairing);
        // A new code replaces the old one.
        let a = s.issue(Role::Camera, t0);
        let b = s.issue(Role::Camera, t0);
        assert!(s.auth(&a, t0).is_err() && s.auth(&b, t0).is_ok());
    }

    #[test]
    fn app_routes_refuse_web_pages_and_the_tunnel() {
        let mut h = HeaderMap::new();
        assert!(from_app(&h), "no Origin: the app's own process or the CLI");
        h.insert(
            header::ORIGIN,
            HeaderValue::from_static("tauri://localhost"),
        );
        assert!(from_app(&h));
        h.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.example"),
        );
        assert!(!from_app(&h));
        let mut t = HeaderMap::new();
        t.insert("cf-connecting-ip", HeaderValue::from_static("1.2.3.4"));
        assert!(!from_app(&t));
    }

    #[test]
    fn offsets_from_wall_clock() {
        assert_eq!(offset_secs(1_000_400, 1_000_000), 0.4);
        assert_eq!(offset_secs(999_750, 1_000_000), -0.25);
    }

    #[test]
    fn qr_encodes() {
        let svg = qr_svg("https://192.168.1.20:5555/phone#t=abc").unwrap();
        assert!(svg.starts_with("<?xml") && svg.contains("<svg"));
    }
}
