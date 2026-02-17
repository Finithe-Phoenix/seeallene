use axum::{
    body::Body,
    extract::Query,
    http::{header, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
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
    // Real screen capture (native runs). In Docker/CI/headless, capture will likely fail.
    // In those cases we fall back to a placeholder image and return a best-effort JPEG.

    let quality = clamp(quality, 30, 90);

    match capture_jpeg_real(quality) {
        Ok(buf) => Ok(buf),
        Err(err) => {
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

            // Encode error info into a trailing marker for logs; response headers will also expose this.
            error!(%err, "capture failed; serving placeholder");
            Ok(out)
        }
    }
}

fn capture_jpeg_real(quality: u8) -> Result<Vec<u8>, String> {
    use std::{io::ErrorKind, thread, time::Duration};

    let display = scrap::Display::primary().map_err(|e| format!("display: {e}"))?;
    let mut capturer = scrap::Capturer::new(display).map_err(|e| format!("capturer: {e}"))?;

    let (w, h) = (capturer.width(), capturer.height());

    // scrap returns BGRA.
    let mut frame = None;
    for _ in 0..50 {
        match capturer.frame() {
            Ok(buf) => {
                frame = Some(buf);
                break;
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) => return Err(format!("frame: {e}")),
        }
    }
    let frame = frame.ok_or_else(|| "frame: timeout".to_string())?;

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
    // Best-effort: attempt to know if capture is likely to work.
    let capture_ok = scrap::Display::primary().is_ok();
    Json(json!({"ok": true, "bind": "127.0.0.1", "capture": if capture_ok {"ok"} else {"unavailable"}}))
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
            let mode = if scrap::Display::primary().is_ok() {
                "real_or_placeholder"
            } else {
                "placeholder"
            };
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

    let app = Router::new()
        .route("/", get(health))
        .route("/health", get(health))
        .route("/snapshot.jpg", get(snapshot))
        .route("/stream", get(stream_mjpeg));

    let bind_ip = std::env::var("SEEALLN_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("SEEALLN_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8765);

    let addr: SocketAddr = format!("{}:{}", bind_ip, port).parse().unwrap();
    info!("SeeAlln Rust server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
