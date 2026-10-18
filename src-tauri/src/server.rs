//! Local HTTP server for the watch page and media. Every URL carries the video's random
//! share id, so a public tunnel only reaches videos someone was given a link to.
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{Path, Query, Request};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::json;
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeFile;

use crate::ai;
use crate::brand::BRAND;
use crate::library;

const WATCH_HTML: &str = include_str!("watch.html");

pub async fn start() -> anyhow::Result<u16> {
    let app = Router::new()
        .route("/v/{share}", get(watch))
        .route("/media/{share}", get(media))
        .route("/raw/{share}", get(raw))
        .route("/preview/{share}", get(preview))
        .route("/thumb/{share}", get(thumb))
        .route("/captions/{share}", get(captions))
        .route("/api/v/{share}", get(meta))
        .route("/api/v/{share}/ask", post(ask))
        // The app window loads media and captions from here. GET only: no other site can POST a question.
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([axum::http::Method::GET]),
        );
    // A fixed port keeps local links stable between launches; fall back to any free port.
    let listener =
        match tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 47821))).await {
            Ok(l) => l,
            Err(_) => tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?,
        };
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(port)
}

fn strip_ext(s: &str) -> &str {
    s.split('.').next().unwrap_or(s)
}

fn find(share: &str) -> Result<library::Video, StatusCode> {
    library::by_share(strip_ext(share)).ok_or(StatusCode::NOT_FOUND)
}

/// Requests through Cloudflare carry cf-connecting-ip; local ones do not.
fn is_public(h: &HeaderMap) -> bool {
    h.contains_key("cf-connecting-ip")
}

async fn watch(Path(share): Path<String>) -> Result<Html<String>, StatusCode> {
    let v = find(&share)?;
    let page = WATCH_HTML
        .replace(
            "__BRAND__",
            &serde_json::to_string(&*BRAND).unwrap_or_default(),
        )
        .replace(
            "__SHARE__",
            &serde_json::to_string(&v.share_id).unwrap_or_default(),
        )
        .replace("__TITLE__", &html_escape(&v.title))
        .replace("__BRAND_NAME__", &html_escape(&BRAND.name));
    Ok(Html(page))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

async fn serve_file(path: std::path::PathBuf, req: Request) -> Response {
    match ServeFile::new(path).oneshot(req).await {
        Ok(r) => r.map(Body::new).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn media(Path(share): Path<String>, req: Request) -> Response {
    match find(&share) {
        Ok(v) => serve_file(library::playable_path(&v.id), req).await,
        Err(s) => s.into_response(),
    }
}

/// The edit without the voice settings, so the app can preview a new gain live.
async fn raw(Path(share): Path<String>, req: Request) -> Response {
    match find(&share) {
        Ok(v) => serve_file(library::video_path(&v.id), req).await,
        Err(s) => s.into_response(),
    }
}

async fn preview(Path(share): Path<String>, req: Request) -> Response {
    match find(&share) {
        Ok(v) => serve_file(library::preview_path(&v.id), req).await,
        Err(s) => s.into_response(),
    }
}

async fn thumb(Path(share): Path<String>, req: Request) -> Response {
    match find(&share) {
        Ok(v) => serve_file(library::thumb_path(&v.id), req).await,
        Err(s) => s.into_response(),
    }
}

#[derive(Deserialize)]
struct CaptionQuery {
    lang: Option<String>,
}

async fn captions(Path(share): Path<String>, Query(q): Query<CaptionQuery>) -> Response {
    let Ok(v) = find(&share) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let body = match q.lang.as_deref() {
        Some(lang) if v.caption_langs.iter().any(|l| l == lang) => {
            std::fs::read_to_string(crate::captions::track_path(&v.id, lang)).unwrap_or_default()
        }
        _ => {
            let words = library::load_transcript(&v.id)
                .map(|t| t.words)
                .unwrap_or_default();
            library::to_vtt(&words)
        }
    };
    ([(header::CONTENT_TYPE, "text/vtt; charset=utf-8")], body).into_response()
}

async fn meta(
    Path(share): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let v = find(&share)?;
    let cues = library::load_transcript(&v.id)
        .map(|t| library::cues(&t.words))
        .unwrap_or_default();
    Ok(Json(json!({
        "title": v.title, "summary": v.summary, "chapters": v.chapters, "duration": v.duration,
        "cues": cues,
        "languages": v.caption_langs.iter().map(|l| json!([l, crate::captions::language_name(l).unwrap_or(l)])).collect::<Vec<_>>(),
        "can_ask": v.has_transcript && (!is_public(&headers) || v.public_questions),
    })))
}

#[derive(Deserialize)]
struct AskBody {
    question: String,
}

static ASKS: Lazy<Mutex<HashMap<String, Vec<Instant>>>> = Lazy::new(Default::default);
const ASKS_PER_HOUR: usize = 20;

async fn ask(Path(share): Path<String>, headers: HeaderMap, Json(body): Json<AskBody>) -> Response {
    let Ok(v) = find(&share) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if is_public(&headers) && !v.public_questions {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error": "The owner has not turned on questions for this video."})),
        )
            .into_response();
    }
    if is_public(&headers) {
        let mut g = ASKS.lock().unwrap();
        let log = g.entry(v.share_id.clone()).or_default();
        log.retain(|t| t.elapsed().as_secs() < 3600);
        if log.len() >= ASKS_PER_HOUR {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({"error": "Too many questions. Try again later."})),
            )
                .into_response();
        }
        log.push(Instant::now());
    }
    let Some(t) = library::load_transcript(&v.id) else {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "This video has no transcript yet."})),
        )
            .into_response();
    };
    match ai::ask(&t, &v.title, &body.question).await {
        Ok(a) => Json(json!(a)).into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
