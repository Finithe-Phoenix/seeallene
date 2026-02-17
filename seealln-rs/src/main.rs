use axum::{
    body::Body,
    extract::Query,
    http::{header, HeaderValue, StatusCode},
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

fn capture_jpeg(_quality: u8) -> Result<Vec<u8>, String> {
    // NOTE: MVP placeholder image. Next step: real cross-platform capture.
    let width = 640;
    let height = 360;
    let mut imgbuf = image::RgbImage::new(width, height);
    for p in imgbuf.pixels_mut() {
        *p = image::Rgb([16, 16, 20]);
    }

    let mut out = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80);
    encoder
        .encode_image(&image::DynamicImage::ImageRgb8(imgbuf))
        .map_err(|e| e.to_string())?;
    Ok(out)
}

async fn health() -> impl IntoResponse {
    Json(json!({"ok": true, "bind": "127.0.0.1"}))
}

async fn snapshot() -> Response {
    match capture_jpeg(75) {
        Ok(buf) => {
            let mut resp = Response::new(Body::from(buf));
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("image/jpeg"),
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
