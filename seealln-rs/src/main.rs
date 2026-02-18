use axum::{
    body::Body,
    extract::Query,
    http::{header, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use bytes::Bytes;
use futures::stream;
use serde::Deserialize;
use serde_json::json;
use std::{
    convert::Infallible,
    net::SocketAddr,
    time::{Duration, Instant},
};
use tracing::{error, info};

mod hands;

#[derive(Debug, Deserialize)]
struct StreamParams {
    fps: Option<f32>,
    q: Option<u8>,
}

fn clamp<T: PartialOrd>(v: T, lo: T, hi: T) -> T {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

fn capture_jpeg(quality: u8) -> Result<Vec<u8>, String> {
    // Real screen capture when enabled; otherwise placeholder.
    // We intentionally keep endpoints stable even when capture is disabled/unavailable.

    let quality = clamp(quality, 30, 90);

    #[cfg(feature = "capture")]
    {
        match capture_jpeg_real(quality) {
            Ok(buf) => return Ok(buf),
            Err(err) => {
                error!(%err, "capture failed; serving placeholder");
            }
        }
    }

    // Fallback placeholder (keeps endpoints stable)
    let width = 640;
    let height = 360;
    let mut imgbuf = image::RgbImage::new(width, height);
    for (i, p) in imgbuf.pixels_mut().enumerate() {
        let x = (i as u32) % width;
        let y = (i as u32) / width;
        let v = (((x ^ y) & 0x3F) as u8).saturating_add(16);
        *p = image::Rgb([v, v, v.saturating_add(8)]);
    }

    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    encoder
        .encode_image(&image::DynamicImage::ImageRgb8(imgbuf))
        .map_err(|e| e.to_string())?;

    Ok(out)
}

#[cfg(feature = "capture")]
fn capture_jpeg_real(quality: u8) -> Result<Vec<u8>, String> {
    use std::{io::ErrorKind, thread, time::Duration};

    let display = scrap::Display::primary().map_err(|e| format!("display: {e}"))?;
    let mut capturer = scrap::Capturer::new(display).map_err(|e| format!("capturer: {e}"))?;

    let (w, h) = (capturer.width(), capturer.height());

    // scrap returns BGRA. We must copy the frame bytes because `frame()` borrows from `capturer`.
    let mut frame_copy: Option<Vec<u8>> = None;
    for _ in 0..50 {
        match capturer.frame() {
            Ok(buf) => {
                frame_copy = Some(buf.to_vec());
                break;
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) => return Err(format!("frame: {e}")),
        }
    }
    let frame = frame_copy.ok_or_else(|| "frame: timeout".to_string())?;

    // Convert BGRA -> RGB
    let mut rgb = vec![0u8; w * h * 3];
    for i in 0..(w * h) {
        let b = frame[i * 4];
        let g = frame[i * 4 + 1];
        let r = frame[i * 4 + 2];
        rgb[i * 3] = r;
        rgb[i * 3 + 1] = g;
        rgb[i * 3 + 2] = b;
    }

    let img = image::RgbImage::from_raw(w as u32, h as u32, rgb)
        .ok_or_else(|| "rgb buffer: invalid".to_string())?;

    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    encoder
        .encode_image(&image::DynamicImage::ImageRgb8(img))
        .map_err(|e| e.to_string())?;

    Ok(out)
}

async fn health() -> impl IntoResponse {
    #[cfg(feature = "capture")]
    let capture = if scrap::Display::primary().is_ok() {
        "ok"
    } else {
        "unavailable"
    };

    #[cfg(not(feature = "capture"))]
    let capture = "disabled";

    let hands = if cfg!(feature = "hands") { "available" } else { "disabled" };
    Json(json!({"ok": true, "bind": "127.0.0.1", "capture": capture, "hands": hands, "hands_policy": {"arming": "required", "confirm_header": "x-seealln-confirm: yes", "rate_limit": {"max_actions": std::env::var("SEEALLN_HANDS_MAX_ACTIONS").ok(), "window_ms": std::env::var("SEEALLN_HANDS_WINDOW_MS").ok()} } }))
}

async fn snapshot() -> Response {
    // We always try to return a JPEG (real capture preferred; placeholder as fallback).
    // Any hard failure returns 500.
    match capture_jpeg(75) {
        Ok(buf) => {
            let mut resp = Response::new(Body::from(buf));
            resp.headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
            // A hint for clients; real/placeholder is inferred from ability to open a Display.
            #[cfg(feature = "capture")]
            let mode = if scrap::Display::primary().is_ok() {
                "real_or_placeholder"
            } else {
                "placeholder"
            };

            #[cfg(not(feature = "capture"))]
            let mode = "placeholder";
            resp.headers_mut().insert(
                HeaderName::from_static("x-seealln-capture"),
                HeaderValue::from_static(mode),
            );
            resp
        }
        Err(err) => {
            error!(%err, "snapshot failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn stream_mjpeg(Query(params): Query<StreamParams>) -> Response {
    let fps = clamp(params.fps.unwrap_or(10.0), 1.0, 15.0);
    let q = clamp(params.q.unwrap_or(60), 30, 85);

    let boundary = "frame";

    let body_stream = stream::unfold(Instant::now(), move |mut last| async move {
        let frame_interval = Duration::from_secs_f32(1.0 / fps);
        let now = Instant::now();
        if now.duration_since(last) < frame_interval {
            tokio::time::sleep(frame_interval - now.duration_since(last)).await;
        }
        last = Instant::now();

        let jpeg = match capture_jpeg(q) {
            Ok(b) => b,
            Err(_) => Vec::new(),
        };

        let mut chunk = Vec::with_capacity(jpeg.len() + 128);
        chunk.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        chunk.extend_from_slice(b"Content-Type: image/jpeg\r\n");
        chunk.extend_from_slice(format!("Content-Length: {}\r\n\r\n", jpeg.len()).as_bytes());
        chunk.extend_from_slice(&jpeg);
        chunk.extend_from_slice(b"\r\n");

        Some((Ok::<Bytes, Infallible>(Bytes::from(chunk)), last))
    });

    let mut resp = Response::new(Body::from_stream(body_stream));
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&format!(
            "multipart/x-mixed-replace; boundary={boundary}"
        ))
        .unwrap(),
    );
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let hands_state = hands::HandsState::new();

    let app = Router::new()
        .route("/", get(health))
        .route("/health", get(health))
        .route("/snapshot.jpg", get(snapshot))
        .route("/stream", get(stream_mjpeg))
        // Hands (input control) - guarded, local-only
        .route("/hands/arm", post(hands::hands_arm))
        .route("/hands/disarm", post(hands::hands_disarm))
        .route("/hands/move", post(hands::hands_move))
        .route("/hands/click", post(hands::hands_click))
        .route("/hands/type", post(hands::hands_type))

        // Safety + scope
        .route("/safety/kill", post(hands::safety_kill))
        .route("/safety/reset", post(hands::safety_reset))
        .route("/safety/status", get(hands::safety_status))
        .route("/scope/set", post(hands::scope_set))
        .with_state(hands_state);

    let bind_ip_raw = std::env::var("SEEALLN_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
    let bind_ip = bind_ip_raw.trim();
    let bind_ip = if bind_ip.is_empty() { "127.0.0.1" } else { bind_ip };

    let port: u16 = std::env::var("SEEALLN_PORT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(8765);

    let addr: SocketAddr = format!("{}:{}", bind_ip, port)
        .parse()
        .unwrap_or_else(|_| {
            // Defensive fallback
            "127.0.0.1:8765".parse().expect("valid fallback socket")
        });
    info!("SeeAlln Rust server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
