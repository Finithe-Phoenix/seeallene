use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct HandsState {
    inner: Arc<Mutex<HandsInner>>,
}

#[derive(Default)]
struct HandsInner {
    armed_until: Option<Instant>,
    token: Option<String>,
    // Simple rate limit: max actions within a window
    window_start: Option<Instant>,
    window_actions: u32,

    // Safety kill switch: when true, all hands actions are forbidden.
    killed: bool,

    // Optional scope/region lock (inclusive min, exclusive max)
    scope: Option<ScopeRect>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct ScopeRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl ScopeRect {
    pub fn clamp_point(&self, x: i32, y: i32) -> (i32, i32) {
        let min_x = self.x;
        let min_y = self.y;
        let max_x = self.x.saturating_add(self.w).saturating_sub(1);
        let max_y = self.y.saturating_add(self.h).saturating_sub(1);
        (x.clamp(min_x, max_x), y.clamp(min_y, max_y))
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x.saturating_add(self.w) && y < self.y.saturating_add(self.h)
    }
}

impl HandsState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_killed(&self) -> bool {
        self.inner.lock().unwrap().killed
    }

    pub fn kill(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.killed = true;
        inner.armed_until = None;
        inner.token = None;
        inner.window_start = None;
        inner.window_actions = 0;
    }

    pub fn reset_kill(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.killed = false;
    }

    pub fn set_scope(&self, scope: Option<ScopeRect>) {
        let mut inner = self.inner.lock().unwrap();
        inner.scope = scope;
    }

    pub fn get_scope(&self) -> Option<ScopeRect> {
        self.inner.lock().unwrap().scope
    }

    pub fn is_armed(&self, token: &str) -> bool {
        let now = Instant::now();
        let inner = self.inner.lock().unwrap();
        match (&inner.armed_until, &inner.token) {
            (Some(until), Some(t)) if now <= *until && t == token => true,
            _ => false,
        }
    }

    pub fn arm(&self, ttl: Duration, token: String) {
        let mut inner = self.inner.lock().unwrap();
        inner.armed_until = Some(Instant::now() + ttl);
        inner.token = Some(token);
    }

    pub fn disarm(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.armed_until = None;
        inner.token = None;
        inner.window_start = None;
        inner.window_actions = 0;
    }

    pub fn consume_action(&self, token: &str) -> Result<(), &'static str> {
        // Enforce kill switch + arming + basic rate limiting to prevent runaway loops.
        if self.is_killed() {
            return Err("killed");
        }
        if !self.is_armed(token) {
            return Err("not armed");
        }

        let max_actions = std::env::var("SEEALLN_HANDS_MAX_ACTIONS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(20);
        let window_ms = std::env::var("SEEALLN_HANDS_WINDOW_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(10_000);

        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();

        let reset = match inner.window_start {
            None => true,
            Some(t0) => now.duration_since(t0) > Duration::from_millis(window_ms),
        };
        if reset {
            inner.window_start = Some(now);
            inner.window_actions = 0;
        }

        if inner.window_actions >= max_actions {
            return Err("rate limited");
        }

        inner.window_actions += 1;
        Ok(())
    }
}

fn require_local_only(headers: &HeaderMap) -> Result<(), (StatusCode, &'static str)> {
    // Bind is localhost by default, but we still add a belt-and-suspenders header check.
    // If user exposes it accidentally, this prevents blind remote control unless they disable it.
    // User can set SEEALLN_ALLOW_REMOTE=1 to bypass (not recommended).
    if std::env::var("SEEALLN_ALLOW_REMOTE").ok().as_deref() == Some("1") {
        return Ok(());
    }

    // Some reverse proxies add X-Forwarded-For. If present, we assume we're being proxied.
    if headers.contains_key("x-forwarded-for") {
        return Err((StatusCode::FORBIDDEN, "proxied requests not allowed"));
    }
    Ok(())
}

fn gen_token() -> String {
    // Simple random token; good enough for local, short-lived arming.
    // NOTE: We avoid adding extra deps for now.
    let t = Instant::now();
    format!("t{}", t.elapsed().as_nanos())
}

#[derive(Debug, Deserialize)]
pub struct ArmParams {
    ttl_ms: Option<u64>,
}

pub async fn hands_arm(
    State(state): State<HandsState>,
    headers: HeaderMap,
    Query(params): Query<ArmParams>,
) -> impl IntoResponse {
    if let Err((code, msg)) = require_local_only(&headers) {
        return (code, Json(json!({"ok": false, "error": msg}))).into_response();
    }

    let ttl = Duration::from_millis(params.ttl_ms.unwrap_or(30_000).clamp(5_000, 300_000));
    let token = gen_token();
    state.arm(ttl, token.clone());

    (StatusCode::OK, Json(json!({"ok": true, "armed": true, "ttl_ms": ttl.as_millis(), "token": token}))).into_response()
}

pub async fn hands_disarm(State(state): State<HandsState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err((code, msg)) = require_local_only(&headers) {
        return (code, Json(json!({"ok": false, "error": msg}))).into_response();
    }
    state.disarm();
    (StatusCode::OK, Json(json!({"ok": true, "armed": false}))).into_response()
}

// Safety endpoints
pub async fn safety_kill(State(state): State<HandsState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err((code, msg)) = require_local_only(&headers) {
        return (code, Json(json!({"ok": false, "error": msg}))).into_response();
    }
    state.kill();
    (StatusCode::OK, Json(json!({"ok": true, "killed": true}))).into_response()
}

pub async fn safety_reset(State(state): State<HandsState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err((code, msg)) = require_local_only(&headers) {
        return (code, Json(json!({"ok": false, "error": msg}))).into_response();
    }

    // Extra confirm gate
    let confirm = headers
        .get("x-seealln-confirm")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("yes"))
        .unwrap_or(false);
    if !confirm {
        return (
            StatusCode::PRECONDITION_REQUIRED,
            Json(json!({"ok": false, "error": "missing x-seealln-confirm: yes"})),
        )
            .into_response();
    }

    state.reset_kill();
    (StatusCode::OK, Json(json!({"ok": true, "killed": false}))).into_response()
}

pub async fn safety_status(State(state): State<HandsState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err((code, msg)) = require_local_only(&headers) {
        return (code, Json(json!({"ok": false, "error": msg}))).into_response();
    }
    (StatusCode::OK, Json(json!({"ok": true, "killed": state.is_killed(), "scope": state.get_scope()}))).into_response()
}

#[derive(Debug, Deserialize)]
pub struct ScopeReq {
    // null to clear
    scope: Option<ScopeRect>,
}

pub async fn scope_set(
    State(state): State<HandsState>,
    headers: HeaderMap,
    Json(req): Json<ScopeReq>,
) -> impl IntoResponse {
    if let Err((code, msg)) = require_local_only(&headers) {
        return (code, Json(json!({"ok": false, "error": msg}))).into_response();
    }

    if let Some(s) = req.scope {
        if s.w <= 0 || s.h <= 0 {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": "invalid scope"})),
            )
                .into_response();
        }
    }

    state.set_scope(req.scope);
    (StatusCode::OK, Json(json!({"ok": true, "scope": state.get_scope()}))).into_response()
}

#[derive(Debug, Deserialize)]
pub struct MoveReq {
    // absolute screen coords
    x: i32,
    y: i32,
    token: String,
}

#[derive(Debug, Deserialize)]
pub struct ClickReq {
    pub button: Option<String>,
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct TypeReq {
    text: String,
    token: String,
}

fn reject_sensitive_text(text: &str) -> bool {
    // Guardrail: if it looks like login/MFA/captcha, bail.
    let t = text.to_lowercase();
    ["password", "contrase", "otp", "2fa", "mfa", "captcha", "verification code", "c	digo"]
        .iter()
        .any(|k| t.contains(k))
}

#[cfg(feature = "hands")]
fn enigo_click(button: Option<&str>) -> Result<(), String> {
    use enigo::{Button, Coordinate, Direction, Enigo, Mouse, Settings};
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    let b = match button.unwrap_or("left") {
        "left" => Button::Left,
        "right" => Button::Right,
        "middle" => Button::Middle,
        _ => return Err("invalid button".to_string()),
    };
    enigo.button(b, Direction::Click).map_err(|e| e.to_string())
}

#[cfg(feature = "hands")]
fn enigo_move(x: i32, y: i32) -> Result<(), String> {
    use enigo::{Coordinate, Enigo, Mouse, Settings};
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| e.to_string())
}

#[cfg(feature = "hands")]
fn enigo_type(text: &str) -> Result<(), String> {
    use enigo::{Enigo, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo.text(text).map_err(|e| e.to_string())
}

pub async fn hands_move(
    State(state): State<HandsState>,
    headers: HeaderMap,
    Json(req): Json<MoveReq>,
) -> impl IntoResponse {
    if let Err((code, msg)) = require_local_only(&headers) {
        return (code, Json(json!({"ok": false, "error": msg}))).into_response();
    }

    if let Err(msg) = state.consume_action(&req.token) {
        return (StatusCode::FORBIDDEN, Json(json!({"ok": false, "error": msg}))).into_response();
    }

    // Guardrail: clamp to a sane range to avoid overflow; actual screen bounds are OS-specific.
    let mut x = req.x.clamp(-10_000, 10_000);
    let mut y = req.y.clamp(-10_000, 10_000);

    // Apply scope (if set), otherwise clamp to main display.
    #[cfg(feature = "hands")]
    {
        use enigo::{Enigo, Mouse, Settings};
        let en = Enigo::new(&Settings::default()).map_err(|e| e.to_string());
        if let Ok(mut enigo) = en {
            if let Ok((w, h)) = enigo.main_display() {
                // screen clamp
                x = x.clamp(0, w.saturating_sub(1));
                y = y.clamp(0, h.saturating_sub(1));
            }
        }

        if let Some(scope) = state.get_scope() {
            let (cx, cy) = scope.clamp_point(x, y);
            x = cx;
            y = cy;
        }
    }

    #[cfg(feature = "hands")]
    match enigo_move(x, y) {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "error": err}))).into_response(),
    }

    #[cfg(not(feature = "hands"))]
    (StatusCode::NOT_IMPLEMENTED, Json(json!({"ok": false, "error": "hands feature disabled"}))).into_response()
}

pub async fn hands_click(
    State(state): State<HandsState>,
    headers: HeaderMap,
    Json(req): Json<ClickReq>,
) -> impl IntoResponse {
    if let Err((code, msg)) = require_local_only(&headers) {
        return (code, Json(json!({"ok": false, "error": msg}))).into_response();
    }

    if let Err(msg) = state.consume_action(&req.token) {
        return (StatusCode::FORBIDDEN, Json(json!({"ok": false, "error": msg}))).into_response();
    }

    // Extra guardrail: require explicit header to reduce accidental clicks
    let confirm = headers
        .get("x-seealln-confirm")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("yes"))
        .unwrap_or(false);

    if !confirm {
        return (
            StatusCode::PRECONDITION_REQUIRED,
            Json(json!({"ok": false, "error": "missing x-seealln-confirm: yes"})),
        )
            .into_response();
    }

    #[cfg(feature = "hands")]
    match enigo_click(req.button.as_deref()) {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "error": err}))).into_response(),
    }

    #[cfg(not(feature = "hands"))]
    (StatusCode::NOT_IMPLEMENTED, Json(json!({"ok": false, "error": "hands feature disabled"}))).into_response()
}

pub async fn hands_type(
    State(state): State<HandsState>,
    headers: HeaderMap,
    Json(req): Json<TypeReq>,
) -> impl IntoResponse {
    if let Err((code, msg)) = require_local_only(&headers) {
        return (code, Json(json!({"ok": false, "error": msg}))).into_response();
    }

    if let Err(msg) = state.consume_action(&req.token) {
        return (StatusCode::FORBIDDEN, Json(json!({"ok": false, "error": msg}))).into_response();
    }

    // Extra guardrail: require explicit header to reduce accidental typing
    let confirm = headers
        .get("x-seealln-confirm")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("yes"))
        .unwrap_or(false);

    if !confirm {
        return (
            StatusCode::PRECONDITION_REQUIRED,
            Json(json!({"ok": false, "error": "missing x-seealln-confirm: yes"})),
        )
            .into_response();
    }

    let text = req.text;

    // Guardrails
    if text.len() > 200 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "text too long (max 200)"})),
        )
            .into_response();
    }
    if reject_sensitive_text(&text) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"ok": false, "error": "looks like login/MFA/CAPTCHA; refusing"})),
        )
            .into_response();
    }

    #[cfg(feature = "hands")]
    match enigo_type(&text) {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "error": err}))).into_response(),
    }

    #[cfg(not(feature = "hands"))]
    (StatusCode::NOT_IMPLEMENTED, Json(json!({"ok": false, "error": "hands feature disabled"}))).into_response()
}
