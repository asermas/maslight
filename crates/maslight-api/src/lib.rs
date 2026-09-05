//! # maslight-api
//!
//! A small REST and WebSocket surface on localhost, for scripts, Stream Deck
//! buttons, Home Assistant, and anything else that wants to drive the lights
//! without going through the interface.
//!
//! Three rules shape it:
//!
//! * **It binds to loopback only.** Nothing on the network can reach it.
//! * **It needs a token.** A machine with several users is still a machine
//!   where one program should not be able to drive another one's lights.
//! * **It is off by default.** An always-listening socket is not something an
//!   ambilight should open without being asked.
//!
//! The engine is reached through the [`ApiBackend`] trait rather than a
//! concrete type, so this crate knows nothing about Tauri or about how the
//! configuration is stored.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use maslight_core::{AppConfig, Rgb8};
use maslight_engine::EngineStatus;
use serde::{Deserialize, Serialize};

/// What the API needs from the application.
pub trait ApiBackend: Send + Sync + 'static {
    fn status(&self) -> EngineStatus;
    fn config(&self) -> AppConfig;
    /// Replace the configuration, persisting it the same way the interface
    /// would.
    fn apply(&self, config: AppConfig) -> Result<(), String>;
    fn set_enabled(&self, enabled: bool) -> Result<(), String>;
    fn set_profile(&self, id: &str) -> Result<(), String>;
    /// Hold every LED at one colour, or `None` to resume.
    fn hold(&self, color: Option<Rgb8>);
    fn identify(&self, index: usize, ms: u64);
}

/// A running server. Dropping it shuts the socket down.
pub struct ApiServer {
    port: u16,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    runtime: Option<tokio::runtime::Runtime>,
}

impl ApiServer {
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Start listening on `127.0.0.1:port`.
    ///
    /// Runs its own small runtime on its own thread, so the engine never waits
    /// on a web request and the application does not have to be async.
    pub fn start(backend: Arc<dyn ApiBackend>, port: u16, token: String) -> Result<Self, String> {
        if token.trim().is_empty() {
            return Err(String::from("the API needs a token"));
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .thread_name("maslight-api")
            .build()
            .map_err(|e| e.to_string())?;

        let state = ApiState { backend, token };
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));

        let listener = runtime
            .block_on(tokio::net::TcpListener::bind(addr))
            .map_err(|e| format!("cannot listen on {addr}: {e}"))?;
        let bound = listener.local_addr().map_err(|e| e.to_string())?.port();

        let (tx, rx) = tokio::sync::oneshot::channel();
        let app = router(state);
        runtime.spawn(async move {
            let server = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = rx.await;
            });
            if let Err(e) = server.await {
                tracing::warn!("api server stopped: {e}");
            }
        });

        tracing::info!("local API listening on http://127.0.0.1:{bound}");
        Ok(Self {
            port: bound,
            shutdown: Some(tx),
            runtime: Some(runtime),
        })
    }
}

impl Drop for ApiServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(runtime) = self.runtime.take() {
            // Give the graceful shutdown a moment, then stop regardless: this
            // runs while the application is closing and must not hang.
            runtime.shutdown_timeout(Duration::from_millis(500));
        }
    }
}

#[derive(Clone)]
struct ApiState {
    backend: Arc<dyn ApiBackend>,
    token: String,
}

fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api/status", get(status))
        .route("/api/config", get(config).put(put_config))
        .route("/api/enabled", post(set_enabled))
        .route("/api/profile", post(set_profile))
        .route("/api/hold", post(hold))
        .route("/api/identify", post(identify))
        .route("/api/stream", get(stream))
        .route("/api/health", get(health))
        .with_state(state)
        // `put` is imported for the route builder above; keep the import used.
        .route("/api/ping", put(health))
}

/// An error the API can report, always as JSON so a client never has to guess.
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(ErrorBody { error: self.1 })).into_response()
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

/// Check the bearer token, in the header or the query string.
///
/// The query string is allowed because a browser WebSocket cannot set headers,
/// and refusing it would mean no `/api/stream` from a page.
fn authorise(state: &ApiState, headers: &HeaderMap, query: &TokenQuery) -> Result<(), ApiError> {
    let from_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    let supplied = from_header.or(query.token.as_deref());
    match supplied {
        Some(value) if value == state.token => Ok(()),
        _ => Err(ApiError(
            StatusCode::UNAUTHORIZED,
            String::from("a valid token is required"),
        )),
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "maslight",
        "version": maslight_core::VERSION,
    }))
}

async fn status(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Result<Json<EngineStatus>, ApiError> {
    authorise(&state, &headers, &query)?;
    Ok(Json(state.backend.status()))
}

async fn config(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Result<Json<AppConfig>, ApiError> {
    authorise(&state, &headers, &query)?;
    Ok(Json(state.backend.config()))
}

async fn put_config(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(config): Json<AppConfig>,
) -> Result<Json<AppConfig>, ApiError> {
    authorise(&state, &headers, &query)?;
    state
        .backend
        .apply(config)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(state.backend.config()))
}

#[derive(Deserialize)]
struct EnabledBody {
    enabled: bool,
}

/// What a command endpoint answers with.
///
/// Deliberately not the engine status: the engine applies a change on its next
/// frame, so a status read straight after a command would report the state
/// from before it. This reports what was actually committed.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Ack {
    enabled: bool,
    active_profile: String,
}

fn ack(state: &ApiState) -> Json<Ack> {
    let config = state.backend.config();
    Json(Ack {
        enabled: config.enabled,
        active_profile: config.active_profile,
    })
}

async fn set_enabled(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(body): Json<EnabledBody>,
) -> Result<Json<Ack>, ApiError> {
    authorise(&state, &headers, &query)?;
    state
        .backend
        .set_enabled(body.enabled)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(ack(&state))
}

#[derive(Deserialize)]
struct ProfileBody {
    id: String,
}

async fn set_profile(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(body): Json<ProfileBody>,
) -> Result<Json<Ack>, ApiError> {
    authorise(&state, &headers, &query)?;
    state
        .backend
        .set_profile(&body.id)
        .map_err(|e| ApiError(StatusCode::NOT_FOUND, e))?;
    Ok(ack(&state))
}

#[derive(Deserialize)]
struct HoldBody {
    /// `#rrggbb`, or `null` to resume normal output.
    color: Option<String>,
}

async fn hold(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(body): Json<HoldBody>,
) -> Result<StatusCode, ApiError> {
    authorise(&state, &headers, &query)?;
    let colour = match body.color.as_deref() {
        None => None,
        Some(value) => Some(parse_hex(value).ok_or_else(|| {
            ApiError(
                StatusCode::BAD_REQUEST,
                format!("{value} is not a #rrggbb colour"),
            )
        })?),
    };
    state.backend.hold(colour);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct IdentifyBody {
    index: usize,
    ms: Option<u64>,
}

async fn identify(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(body): Json<IdentifyBody>,
) -> Result<StatusCode, ApiError> {
    authorise(&state, &headers, &query)?;
    state.backend.identify(body.index, body.ms.unwrap_or(1200));
    Ok(StatusCode::NO_CONTENT)
}

/// Status frames over a WebSocket, ten a second.
///
/// Polling `/api/status` works, but a dashboard that wants the live strip
/// should not have to hammer it.
async fn stream(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    authorise(&state, &headers, &query)?;
    Ok(upgrade.on_upgrade(move |socket| push_status(socket, state)))
}

async fn push_status(mut socket: WebSocket, state: ApiState) {
    let mut ticker = tokio::time::interval(Duration::from_millis(100));
    loop {
        ticker.tick().await;
        let status = state.backend.status();
        let Ok(text) = serde_json::to_string(&status) else {
            continue;
        };
        if socket.send(Message::Text(text.into())).await.is_err() {
            break;
        }
    }
}

/// Parse `#rrggbb`.
pub fn parse_hex(value: &str) -> Option<Rgb8> {
    let v = value.trim().trim_start_matches('#');
    if v.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(v, 16).ok()?;
    Some(Rgb8::new(
        ((n >> 16) & 0xff) as u8,
        ((n >> 8) & 0xff) as u8,
        (n & 0xff) as u8,
    ))
}

/// A token to put in the configuration the first time the API is enabled.
///
/// Not cryptographic randomness: this guards a loopback socket against other
/// programs on the same machine, and the system clock plus the process id is
/// enough entropy for that. It is regenerated whenever the user asks.
pub fn generate_token() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let mut value = now ^ (pid << 64) ^ 0x9e37_79b9_7f4a_7c15;
    let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut out = String::with_capacity(32);
    for _ in 0..32 {
        // A xorshift is plenty for spreading the seed across the characters.
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        out.push(alphabet[(value % alphabet.len() as u128) as usize] as char);
    }
    out
}
