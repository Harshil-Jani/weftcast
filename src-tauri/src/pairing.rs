//! Pairing a phone over Wi-Fi: a one-time token in a QR code (or a 6-digit code), remembered
//! phones, the phone page, WebRTC signalling, the remote control, and track upload.
//!
//! How the phone reaches the Mac: the page is served through the app's temporary public link (a
//! Cloudflare quick tunnel to the local server), so the phone gets a valid HTTPS certificate and no
//! warning; browsers only allow the camera on a secure page. Only signalling goes that way: the
//! video itself goes phone to Mac over WebRTC with local network (host) candidates only, so it
//! stays on the Wi-Fi. Offline, the page is served on the LAN with a certificate this Mac makes for
//! itself, and the phone asks once to trust it.
//!
//! Two sides, two sets of routes:
//! - phone routes (`/phone`, `/p`, `/api/phone/*`) need the pairing token, the 6-digit code, or a
//!   remembered phone's device token.
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
    /// Remote only: start, pause and stop from the phone; no camera.
    Remote,
}

/// An unused token expires after this; a connected phone stays paired until a recording ends.
pub const UNUSED_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug)]
pub struct Pairing {
    /// Public id of this session, for the app's windows (never the token).
    pub id: String,
    token: String,
    /// 6 digits, for typing on the phone when the camera cannot scan.
    pub code: String,
    code_failures: u8,
    pub role: Role,
    /// Bumped when the role changes, so a connected phone reconnects for its new job.
    pub epoch: u32,
    issued: Instant,
    /// Last time the phone checked in (it polls while its page is open).
    seen: Instant,
    connected: bool,
    pub device: Option<String>,
    /// The remembered phone this pairing belongs to.
    pub phone_id: Option<String>,
    offer: Option<String>,
    answer: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum AuthError {
    NoPairing,
    WrongToken,
    Expired,
    /// Too many wrong codes: the code stops working until a new one is shown.
    Locked,
    /// Already as many phones as one session takes.
    Full,
}

/// Wrong 6-digit codes allowed before the code stops working.
pub const CODE_TRIES: u8 = 5;

/// Phones in one session at most: a face cam, a second angle, and remotes.
pub const MAX_PHONES: usize = 4;

/// A connected phone that stops checking in for this long has closed its page: it drops out.
pub const STALE: Duration = Duration::from_secs(20);

/// The phones paired right now. At most one is waiting (the code on screen); the rest are connected.
#[derive(Default)]
pub struct Store {
    sessions: Vec<Pairing>,
}

/// Compares without stopping at the first difference, so timing reveals nothing about the token.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// The job a phone gets when it connects: the one asked for if free, else the other camera job,
/// else Remote. Face cam and second angle are one phone each; any number of phones can be remotes.
pub fn assign_role(wanted: Role, taken: &[Role]) -> Role {
    let free = |r: Role| r == Role::Remote || !taken.contains(&r);
    if free(wanted) {
        return wanted;
    }
    [Role::Camera, Role::Angle]
        .into_iter()
        .find(|r| free(*r))
        .unwrap_or(Role::Remote)
}

fn new_pairing(role: Role, now: Instant) -> Pairing {
    Pairing {
        id: uuid::Uuid::new_v4().simple().to_string()[..10].to_string(),
        token: uuid::Uuid::new_v4().simple().to_string(),
        code: format!("{:06}", rand::random_range(0..1_000_000u32)),
        code_failures: 0,
        role,
        epoch: 0,
        issued: now,
        seen: now,
        connected: false,
        device: None,
        phone_id: None,
        offer: None,
        answer: None,
    }
}

impl Store {
    /// A new code for the next phone; it replaces a code nobody used, and keeps connected phones.
    pub fn issue(&mut self, role: Role, now: Instant) -> String {
        self.prune(now);
        self.sessions.retain(|p| p.connected);
        let p = new_pairing(role, now);
        let token = p.token.clone();
        self.sessions.push(p);
        token
    }

    /// The code on screen, waiting for a phone.
    pub fn pending(&self) -> Option<&Pairing> {
        self.sessions.iter().find(|p| !p.connected)
    }

    pub fn sessions(&self) -> impl Iterator<Item = &Pairing> {
        self.sessions.iter()
    }

    fn taken_except(&self, id: &str) -> Vec<Role> {
        self.sessions
            .iter()
            .filter(|p| p.connected && p.id != id)
            .map(|p| p.role)
            .collect()
    }

    /// Swaps the 6-digit code for the pairing token.
    pub fn redeem(&mut self, code: &str, now: Instant) -> Result<String, AuthError> {
        let i = self
            .sessions
            .iter()
            .position(|p| !p.connected)
            .ok_or(AuthError::NoPairing)?;
        let p = &mut self.sessions[i];
        if p.code_failures >= CODE_TRIES {
            return Err(AuthError::Locked);
        }
        if now.duration_since(p.issued) > UNUSED_TTL {
            self.sessions.remove(i);
            return Err(AuthError::Expired);
        }
        let digits: String = code.chars().filter(char::is_ascii_digit).collect();
        if !same(&digits, &p.code) {
            p.code_failures += 1;
            return Err(if p.code_failures >= CODE_TRIES {
                AuthError::Locked
            } else {
                AuthError::WrongToken
            });
        }
        Ok(p.token.clone())
    }

    /// The waiting code was used: the phone is connected, with a job that does not clash.
    pub fn connect(
        &mut self,
        token: &str,
        device: &str,
        now: Instant,
    ) -> Result<&mut Pairing, AuthError> {
        self.prune(now);
        let id = self.auth(token, now)?.id.clone();
        if !self.sessions.iter().any(|p| p.id == id && p.connected)
            && self.sessions.iter().filter(|p| p.connected).count() >= MAX_PHONES
        {
            return Err(AuthError::Full);
        }
        let taken = self.taken_except(&id);
        let p = self
            .sessions
            .iter_mut()
            .find(|p| p.id == id)
            .expect("authenticated above");
        if !p.connected {
            p.role = assign_role(p.role, &taken);
        }
        p.connected = true;
        p.device = Some(device.to_string());
        Ok(p)
    }

    /// A remembered phone came back: a fresh session for its last job (or a free one), alongside
    /// the phones already connected. Opening the page twice replaces its own older session.
    pub fn resume(&mut self, phone: &Phone, now: Instant) -> Result<String, AuthError> {
        self.prune(now);
        self.sessions
            .retain(|p| p.phone_id.as_deref() != Some(phone.id.as_str()));
        if self.sessions.iter().filter(|p| p.connected).count() >= MAX_PHONES {
            return Err(AuthError::Full);
        }
        let taken: Vec<Role> = self
            .sessions
            .iter()
            .filter(|p| p.connected)
            .map(|p| p.role)
            .collect();
        let mut p = new_pairing(assign_role(phone.role, &taken), now);
        p.connected = true;
        p.device = Some(phone.name.clone());
        p.phone_id = Some(phone.id.clone());
        let token = p.token.clone();
        self.sessions.push(p);
        Ok(token)
    }

    /// Changes one phone's job. Face cam and second angle swap if another phone has the one asked
    /// for. Every phone whose job changed reconnects for it.
    pub fn set_role(&mut self, id: &str, role: Role) {
        let Some(old) = self.sessions.iter().find(|p| p.id == id).map(|p| p.role) else {
            return;
        };
        if old == role {
            return;
        }
        let bump = |p: &mut Pairing, r: Role| {
            p.role = r;
            p.epoch += 1;
            p.offer = None;
            p.answer = None;
        };
        if role != Role::Remote
            && let Some(other) = self
                .sessions
                .iter_mut()
                .find(|p| p.id != id && p.connected && p.role == role)
        {
            bump(other, old);
        }
        if let Some(p) = self.sessions.iter_mut().find(|p| p.id == id) {
            bump(p, role);
        }
    }

    pub fn is_connected(&self) -> bool {
        self.sessions.iter().any(|p| p.connected)
    }

    pub fn auth(&mut self, token: &str, now: Instant) -> Result<&mut Pairing, AuthError> {
        let i = self
            .sessions
            .iter()
            .position(|p| same(&p.token, token))
            .ok_or(if self.sessions.is_empty() {
                AuthError::NoPairing
            } else {
                AuthError::WrongToken
            })?;
        let p = &self.sessions[i];
        let gone = if p.connected {
            now.duration_since(p.seen) >= STALE
        } else {
            now.duration_since(p.issued) > UNUSED_TTL
        };
        if gone {
            self.sessions.remove(i);
            return Err(AuthError::Expired);
        }
        self.sessions[i].seen = now;
        Ok(&mut self.sessions[i])
    }

    /// Drops connected phones that closed their page (no check-in for a while).
    pub fn prune(&mut self, now: Instant) {
        self.sessions
            .retain(|p| !p.connected || now.duration_since(p.seen) < STALE);
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Pairing> {
        self.sessions.iter_mut().find(|p| p.id == id)
    }

    /// Disconnects one phone.
    pub fn drop_session(&mut self, id: &str) {
        self.sessions.retain(|p| p.id != id);
    }

    /// Disconnects the phones of a remembered phone that was forgotten.
    pub fn drop_phone(&mut self, phone_id: &str) {
        self.sessions
            .retain(|p| p.phone_id.as_deref() != Some(phone_id));
    }

    /// Called when a recording ends (saved or discarded): every token stops working.
    pub fn end(&mut self) {
        self.sessions.clear();
    }

    /// The code on screen, else the first phone: for callers that only care about one.
    pub fn current(&self) -> Option<&Pairing> {
        self.pending().or_else(|| self.sessions.first())
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

/// Ends the current session (a remembered phone can start a new one by opening the page again).
pub fn end_pairing() {
    PAIR.lock().unwrap().end();
}

// ---------- remembered phones ----------

/// A phone that paired once. Its device token is stored on the phone; only a hash is kept here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Phone {
    pub id: String,
    pub name: String,
    pub role: Role,
    token_sha256: String,
    pub created_ms: u64,
    pub last_seen_ms: u64,
}

/// A remembered phone that is not used for this long is forgotten.
pub const PHONE_TTL_MS: u64 = 90 * 24 * 3600 * 1000;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Phones {
    pub list: Vec<Phone>,
}

fn sha(s: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Phones {
    /// Remembers a phone; returns its id and the device token to keep on the phone.
    pub fn issue(&mut self, name: &str, role: Role, now_ms: u64) -> (String, String) {
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        self.list.push(Phone {
            id: id.clone(),
            name: name.chars().take(60).collect(),
            role,
            token_sha256: sha(&token),
            created_ms: now_ms,
            last_seen_ms: now_ms,
        });
        (id, token)
    }

    /// The phone for a device token, if it is still remembered (and not idle for too long).
    pub fn auth(&mut self, token: &str, now_ms: u64) -> Option<Phone> {
        self.list
            .retain(|p| now_ms.saturating_sub(p.last_seen_ms) <= PHONE_TTL_MS);
        let h = sha(token);
        let p = self.list.iter_mut().find(|p| same(&p.token_sha256, &h))?;
        p.last_seen_ms = now_ms;
        Some(p.clone())
    }

    pub fn set_role(&mut self, id: &str, role: Role) {
        if let Some(p) = self.list.iter_mut().find(|p| p.id == id) {
            p.role = role;
        }
    }

    /// Forget: the phone's token stops working at once.
    pub fn revoke(&mut self, id: &str) -> bool {
        let n = self.list.len();
        self.list.retain(|p| p.id != id);
        self.list.len() != n
    }
}

/// Kept next to the app's settings, in the library folder.
fn phones_file() -> PathBuf {
    library::root().join(".phones.json")
}

static PHONES: Lazy<Mutex<Phones>> = Lazy::new(|| {
    Mutex::new(
        std::fs::read(phones_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default(),
    )
});

fn save_phones(p: &Phones) {
    let f = phones_file();
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(b) = serde_json::to_vec_pretty(p) {
        let _ = std::fs::write(f, b);
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------- phone routes ----------

const PHONE_HTML: &str = include_str!("phone.html");

#[derive(Deserialize)]
struct TokenQ {
    token: String,
}

fn denied(e: AuthError) -> Response {
    let (msg, kind) = match e {
        AuthError::NoPairing | AuthError::Expired => (
            "This connection has ended. Open Connect Phone on your Mac and scan the code again.",
            "ended",
        ),
        AuthError::WrongToken => (
            "That code did not match. Check the code on your Mac and try again.",
            "wrong",
        ),
        AuthError::Locked => (
            "Too many tries. Close and reopen Connect Phone on your Mac for a new code.",
            "locked",
        ),
        AuthError::Full => (
            "Four phones are already connected. Disconnect one on your Mac, then try again.",
            "full",
        ),
    };
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": msg, "kind": kind})),
    )
        .into_response()
}

async fn phone_page() -> Response {
    let brand = json!({"name": BRAND.name, "accent": BRAND.accent}).to_string();
    let html = PHONE_HTML
        .replace("__BRAND__", &brand)
        .replace("__BRAND_NAME__", &BRAND.name)
        .replace("__ACCENT__", &BRAND.accent);
    // Never cached: a home-screen icon must always load the current page.
    ([(header::CACHE_CONTROL, "no-store")], Html(html)).into_response()
}

/// Lets the phone keep the page on its home screen, named and coloured from brand.json.
async fn manifest() -> Response {
    let m = json!({
        "name": BRAND.name, "short_name": BRAND.name, "start_url": "/phone", "scope": "/",
        "display": "standalone", "background_color": "#1e1e1e", "theme_color": BRAND.accent,
        "icons": [
            {"src": "/icon-256.png", "sizes": "256x256", "type": "image/png"},
            {"src": "/icon-512.png", "sizes": "512x512", "type": "image/png"}
        ]
    });
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        Json(m),
    )
        .into_response()
}

fn png(bytes: &'static [u8]) -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    )
        .into_response()
}

async fn icon_256() -> Response {
    png(include_bytes!("../icons/128x128@2x.png"))
}

async fn icon_512() -> Response {
    png(include_bytes!("../icons/icon.png"))
}

#[derive(Deserialize)]
struct Hello {
    /// From the QR code or the 6-digit code.
    #[serde(default)]
    token: Option<String>,
    /// A remembered phone's own token, kept on the phone.
    #[serde(default)]
    device_token: Option<String>,
    device: String,
}

/// The phone says hello with a session token (first time) or its device token (remembered).
/// It gets back a session token, its job, and (first time) a device token to keep.
async fn hello(Json(h): Json<Hello>) -> Response {
    let now = Instant::now();
    let name: String = h.device.chars().take(60).collect();
    let mut s = PAIR.lock().unwrap();
    let mut phones = PHONES.lock().unwrap();
    if let Some(token) = &h.token {
        let p = match s.connect(token, &name, now) {
            Ok(p) => p,
            Err(e) => return denied(e),
        };
        // Remember this phone, unless it already is.
        let known = h
            .device_token
            .as_deref()
            .and_then(|t| phones.auth(t, now_ms()));
        let (id, keep) = match known {
            Some(ph) => (ph.id, None),
            None => {
                let (id, t) = phones.issue(&name, p.role, now_ms());
                (id, Some(t))
            }
        };
        p.phone_id = Some(id);
        let out = json!({"session": token, "role": p.role, "epoch": p.epoch, "device_token": keep});
        save_phones(&phones);
        return Json(out).into_response();
    }
    match h.device_token.as_deref().and_then(|t| phones.auth(t, now_ms())) {
        Some(ph) => match s.resume(&ph, now) {
            Ok(session) => {
                save_phones(&phones);
                let role = s.auth(&session, now).map(|p| p.role).unwrap_or(ph.role);
                Json(json!({"session": session, "role": role, "epoch": 0, "device_token": null})).into_response()
            }
            Err(e) => denied(e),
        },
        None => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "This phone was forgotten on the Mac. Scan the code in Connect Phone to connect it again.", "kind": "forgotten"})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct Code {
    code: String,
}

async fn code(Json(c): Json<Code>) -> Response {
    match PAIR.lock().unwrap().redeem(&c.code, Instant::now()) {
        Ok(token) => Json(json!({"token": token})).into_response(),
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
        .map(|p| (p.role, p.epoch));
    match ok {
        Ok((role, epoch)) => {
            let mut st = STATUS.lock().unwrap().clone();
            st["role"] = json!(role);
            st["epoch"] = json!(epoch);
            Json(st).into_response()
        }
        Err(e) => denied(e),
    }
}

#[derive(Deserialize)]
struct PhoneRole {
    token: String,
    role: Role,
}

/// The phone picks its own job on its page; the Mac view shows the change.
async fn phone_role(Json(r): Json<PhoneRole>) -> Response {
    let mut s = PAIR.lock().unwrap();
    let id = match s.auth(&r.token, Instant::now()) {
        Ok(p) => p.id.clone(),
        Err(e) => return denied(e),
    };
    s.set_role(&id, r.role);
    let phone_id = s.get_mut(&id).and_then(|p| p.phone_id.clone());
    drop(s);
    if let Some(pid) = phone_id {
        let mut ph = PHONES.lock().unwrap();
        ph.set_role(&pid, r.role);
        save_phones(&ph);
    }
    StatusCode::NO_CONTENT.into_response()
}

pub fn phone_routes() -> Router {
    Router::new()
        .route("/phone", get(phone_page))
        .route("/p", get(phone_page))
        .route("/manifest.webmanifest", get(manifest))
        .route("/icon-256.png", get(icon_256))
        .route("/icon-512.png", get(icon_512))
        .route("/brand.png", get(brand_png))
        .route("/api/phone/hello", post(hello))
        .route("/api/phone/code", post(code))
        .route("/api/phone/offer", post(offer))
        .route("/api/phone/answer", get(answer_for_phone))
        .route("/api/phone/remote", post(remote))
        .route("/api/phone/status", get(status_for_phone))
        .route("/api/phone/role", post(phone_role))
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
    let mut s = PAIR.lock().unwrap();
    s.prune(Instant::now());
    let phones: Vec<Value> = s
        .sessions()
        .filter(|p| p.connected)
        .map(|p| json!({"id": p.id, "role": p.role, "epoch": p.epoch, "device": p.device, "offer": p.offer, "answered": p.answer.is_some()}))
        .collect();
    let pending = s.pending().map(|p| json!({"id": p.id, "role": p.role}));
    let names: Vec<&str> = s
        .sessions()
        .filter(|p| p.connected)
        .filter_map(|p| p.device.as_deref())
        .collect();
    Json(json!({
        "paired": pending.is_some() || !phones.is_empty(),
        "connected": !phones.is_empty(),
        "device": if names.is_empty() { Value::Null } else { json!(names.join(", ")) },
        "pending": pending,
        "phones": phones,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct AppAnswer {
    /// Which phone; without it, the first one waiting for an answer.
    #[serde(default)]
    id: Option<String>,
    sdp: String,
}

async fn app_answer(h: HeaderMap, Json(a): Json<AppAnswer>) -> Response {
    if !from_app(&h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut s = PAIR.lock().unwrap();
    let id = a.id.clone().or_else(|| {
        s.sessions()
            .find(|p| p.offer.is_some() && p.answer.is_none())
            .map(|p| p.id.clone())
    });
    match id.and_then(|id| s.get_mut(&id)) {
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

async fn app_phones(h: HeaderMap) -> Response {
    if !from_app(&h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut ph = PHONES.lock().unwrap();
    ph.list
        .retain(|p| now_ms().saturating_sub(p.last_seen_ms) <= PHONE_TTL_MS);
    let list: Vec<Value> = ph.list.iter().map(|p| json!({"id": p.id, "name": p.name, "role": p.role, "created_ms": p.created_ms, "last_seen_ms": p.last_seen_ms})).collect();
    Json(json!({"phones": list})).into_response()
}

#[derive(Deserialize)]
struct Forget {
    id: String,
}

async fn app_forget(h: HeaderMap, Json(f): Json<Forget>) -> Response {
    if !from_app(&h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut ph = PHONES.lock().unwrap();
    let gone = ph.revoke(&f.id);
    save_phones(&ph);
    // A forgotten phone that is connected right now is disconnected too.
    let mut s = PAIR.lock().unwrap();
    s.drop_phone(&f.id);
    Json(json!({"forgotten": gone})).into_response()
}

#[derive(Deserialize)]
struct RoleBody {
    /// Which phone; without it, the code on screen (the job the next phone gets).
    #[serde(default)]
    id: Option<String>,
    role: Role,
}

async fn app_role(h: HeaderMap, Json(r): Json<RoleBody>) -> Response {
    if !from_app(&h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut s = PAIR.lock().unwrap();
    let Some(id) = r.id.clone().or_else(|| s.current().map(|p| p.id.clone())) else {
        return StatusCode::CONFLICT.into_response();
    };
    s.set_role(&id, r.role);
    if let Some(id) = s.get_mut(&id).and_then(|p| p.phone_id.clone()) {
        let mut ph = PHONES.lock().unwrap();
        ph.set_role(&id, r.role);
        save_phones(&ph);
    }
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct Disconnect {
    id: String,
}

/// Disconnects one phone; it stays remembered and can come back from its home-screen icon.
async fn app_disconnect(h: HeaderMap, Json(d): Json<Disconnect>) -> Response {
    if !from_app(&h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    PAIR.lock().unwrap().drop_session(&d.id);
    StatusCode::NO_CONTENT.into_response()
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
        .route("/api/pair/phones", get(app_phones))
        .route("/api/pair/phones/forget", post(app_forget))
        .route("/api/pair/role", post(app_role))
        .route("/api/pair/disconnect", post(app_disconnect))
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
    // Two TLS backends are linked (ring and aws-lc-rs) through other crates, so rustls cannot pick
    // one by itself and would panic here; choose ring once for the whole process.
    let _ = rustls::crypto::ring::default_provider().install_default();
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
        // Once connected it lasts as long as the phone keeps checking in, however long that takes.
        let tok = s.issue(Role::Angle, t0);
        s.auth(&tok, t0).unwrap().connected = true;
        let mut at = t0;
        while at < t0 + UNUSED_TTL * 4 {
            at += Duration::from_secs(10);
            assert!(s.auth(&tok, at).is_ok());
        }
        s.end();
        assert_eq!(s.auth(&tok, t0).unwrap_err(), AuthError::NoPairing);
        // A new code replaces the old one.
        let a = s.issue(Role::Camera, t0);
        let b = s.issue(Role::Camera, t0);
        assert!(s.auth(&a, t0).is_err() && s.auth(&b, t0).is_ok());
    }

    #[test]
    fn six_digit_codes_redeem_and_lock() {
        let mut s = Store::default();
        let t0 = Instant::now();
        let tok = s.issue(Role::Camera, t0);
        let code = s.current().unwrap().code.clone();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
        // Spaces are fine ("123 456").
        let spaced = format!("{} {}", &code[..3], &code[3..]);
        assert_eq!(s.redeem(&spaced, t0).unwrap(), tok);
        let wrong = if code == "000000" { "111111" } else { "000000" };
        for _ in 0..CODE_TRIES - 1 {
            assert_eq!(s.redeem(wrong, t0).unwrap_err(), AuthError::WrongToken);
        }
        assert_eq!(s.redeem(wrong, t0).unwrap_err(), AuthError::Locked);
        assert_eq!(
            s.redeem(&code, t0).unwrap_err(),
            AuthError::Locked,
            "locked even for the right code"
        );
        s.issue(Role::Camera, t0);
        let fresh = s.current().unwrap().code.clone();
        assert!(s.redeem(&fresh, t0).is_ok(), "a new code works");
    }

    #[test]
    fn remembered_phones_issue_revoke_and_expire() {
        let mut ph = Phones::default();
        let day = 24 * 3600 * 1000;
        let (id, token) = ph.issue("iPhone", Role::Angle, 1_000);
        assert_eq!(token.len(), 64);
        assert!(
            !serde_json::to_string(&ph).unwrap().contains(&token),
            "only a hash is stored"
        );
        let p = ph.auth(&token, 1_000 + day).unwrap();
        assert_eq!((p.name.as_str(), p.role), ("iPhone", Role::Angle));
        assert!(ph.auth("not-a-token", 2_000).is_none());
        // Used within 90 days: still remembered, and the clock restarts.
        assert!(ph.auth(&token, 1_000 + day + PHONE_TTL_MS).is_some());
        // Idle longer than that: forgotten.
        assert!(
            ph.auth(&token, 1_000 + day + 2 * PHONE_TTL_MS + 1)
                .is_none()
        );
        assert!(ph.list.is_empty());
        // Forget revokes at once.
        let (id2, t2) = ph.issue("Pixel", Role::Camera, 5);
        assert!(ph.revoke(&id2) && !ph.revoke(&id2) && !ph.revoke(&id));
        assert!(ph.auth(&t2, 6).is_none());
        // A remembered phone resumes with a fresh session for its last job.
        let mut s = Store::default();
        let (_, t3) = ph.issue("iPad", Role::Angle, 7);
        let phone = ph.auth(&t3, 8).unwrap();
        let session = s.resume(&phone, Instant::now()).unwrap();
        let cur = s.current().unwrap();
        assert!(cur.connected && cur.role == Role::Angle && cur.device.as_deref() == Some("iPad"));
        assert!(s.auth(&session, Instant::now()).is_ok());
    }

    #[test]
    fn role_change_restarts_signalling() {
        let mut s = Store::default();
        s.issue(Role::Camera, Instant::now());
        let id = s.current().unwrap().id.clone();
        s.set_role(&id, Role::Angle);
        let p = s.current().unwrap();
        assert_eq!((p.role, p.epoch), (Role::Angle, 1));
        s.set_role(&id, Role::Angle);
        assert_eq!(s.current().unwrap().epoch, 1, "same role: nothing to redo");
    }

    #[test]
    fn roles_are_assigned_without_clashes() {
        use Role::*;
        assert_eq!(assign_role(Camera, &[]), Camera);
        assert_eq!(
            assign_role(Camera, &[Camera]),
            Angle,
            "face cam taken: second angle"
        );
        assert_eq!(assign_role(Angle, &[Angle]), Camera);
        assert_eq!(
            assign_role(Camera, &[Camera, Angle]),
            Remote,
            "both cameras taken: remote"
        );
        assert_eq!(
            assign_role(Remote, &[]),
            Remote,
            "asked for remote: stays remote"
        );
        assert_eq!(
            assign_role(Remote, &[Remote, Remote]),
            Remote,
            "any number of remotes"
        );
    }

    #[test]
    fn several_phones_join_one_session() {
        let now = Instant::now();
        let mut s = Store::default();
        // First phone: the face cam.
        let t1 = s.issue(Role::Camera, now);
        assert_eq!(s.connect(&t1, "iPhone", now).unwrap().role, Role::Camera);
        // A new code for the next phone keeps the first one connected.
        let t2 = s.issue(Role::Camera, now);
        assert!(s.auth(&t1, now).is_ok(), "the first phone keeps working");
        assert_eq!(
            s.connect(&t2, "iPad", now).unwrap().role,
            Role::Angle,
            "face cam is taken"
        );
        let t3 = s.issue(Role::Camera, now);
        assert_eq!(s.connect(&t3, "Pixel", now).unwrap().role, Role::Remote);
        assert_eq!(s.sessions().filter(|p| p.connected).count(), 3);
        // Making the remote the face cam swaps jobs with the current face cam.
        let id1 = s.auth(&t1, now).unwrap().id.clone();
        let id3 = s.auth(&t3, now).unwrap().id.clone();
        s.set_role(&id3, Role::Camera);
        assert_eq!(s.auth(&t3, now).unwrap().role, Role::Camera);
        assert_eq!(s.auth(&t1, now).unwrap().role, Role::Remote);
        assert!(
            s.auth(&t1, now).unwrap().epoch > 0,
            "the phone that moved reconnects"
        );
        // One unused code at a time: a newer code replaces the older one.
        let old = s.issue(Role::Camera, now);
        let newer = s.issue(Role::Camera, now);
        assert_eq!(s.auth(&old, now).unwrap_err(), AuthError::WrongToken);
        assert!(s.auth(&newer, now).is_ok());
        // A fourth phone fits; a fifth does not.
        assert!(s.connect(&newer, "Galaxy", now).is_ok());
        let t5 = s.issue(Role::Camera, now);
        assert_eq!(s.connect(&t5, "Extra", now).unwrap_err(), AuthError::Full);
        // Disconnecting one phone leaves the others.
        s.drop_session(&id1);
        assert!(s.auth(&t1, now).is_err() && s.auth(&t3, now).is_ok());
        // A recording ending stops them all.
        s.end();
        assert!(!s.is_connected());
    }

    #[test]
    fn phones_that_close_their_page_drop_out() {
        let t0 = Instant::now();
        let mut s = Store::default();
        let a = s.issue(Role::Camera, t0);
        s.connect(&a, "iPhone", t0).unwrap();
        let b = s.issue(Role::Camera, t0);
        s.connect(&b, "iPad", t0).unwrap();
        // Phone a keeps checking in; phone b closed its page.
        assert!(s.auth(&a, t0 + STALE - Duration::from_secs(1)).is_ok());
        let later = t0 + STALE + Duration::from_secs(1);
        s.prune(later);
        assert!(s.auth(&a, later).is_ok(), "still checking in");
        assert_eq!(
            s.auth(&b, later).unwrap_err(),
            AuthError::WrongToken,
            "gone"
        );
        assert_eq!(
            s.sessions().filter(|p| p.connected).count(),
            1,
            "its slot is free again"
        );
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
