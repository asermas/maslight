//! End to end tests against a real socket.
//!
//! The server is started for real and driven over TCP, because the things
//! worth checking here are the ones a unit test cannot see: that it binds to
//! loopback, that the token is actually enforced, and that the bodies are the
//! shape a script would expect.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use maslight_api::{ApiBackend, ApiServer};
use maslight_core::{AppConfig, Rgb8};
use maslight_engine::EngineStatus;

#[derive(Default)]
struct FakeBackend {
    enabled: AtomicBool,
    holds: AtomicUsize,
    identifies: AtomicUsize,
    profile: parking_lot_lite::Mutex<String>,
}

/// A three line mutex so the test needs no extra dependency.
mod parking_lot_lite {
    pub struct Mutex<T>(std::sync::Mutex<T>);

    impl<T: Default> Default for Mutex<T> {
        fn default() -> Self {
            Self(std::sync::Mutex::new(T::default()))
        }
    }

    impl<T: Clone> Mutex<T> {
        pub fn get(&self) -> T {
            self.0.lock().unwrap().clone()
        }
        pub fn set(&self, value: T) {
            *self.0.lock().unwrap() = value;
        }
    }
}

impl ApiBackend for FakeBackend {
    fn status(&self) -> EngineStatus {
        EngineStatus {
            running: true,
            enabled: self.enabled.load(Ordering::SeqCst),
            profile: String::from("Desk"),
            profile_id: self.profile.get(),
            led_count: 4,
            leds: vec![Rgb8::new(1, 2, 3); 4],
            ..Default::default()
        }
    }

    fn config(&self) -> AppConfig {
        // The fake mirrors the real backend: the config is the authority for
        // what was committed, and the status is the engine catching up.
        AppConfig {
            enabled: self.enabled.load(Ordering::SeqCst),
            active_profile: self.profile.get(),
            ..Default::default()
        }
    }

    fn apply(&self, _config: AppConfig) -> Result<(), String> {
        Ok(())
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        self.enabled.store(enabled, Ordering::SeqCst);
        Ok(())
    }

    fn set_profile(&self, id: &str) -> Result<(), String> {
        if id == "missing" {
            return Err(String::from("no such profile"));
        }
        self.profile.set(id.to_string());
        Ok(())
    }

    fn hold(&self, _color: Option<Rgb8>) {
        self.holds.fetch_add(1, Ordering::SeqCst);
    }

    fn identify(&self, _index: usize, _ms: u64) {
        self.identifies.fetch_add(1, Ordering::SeqCst);
    }
}

struct Reply {
    status: u16,
    body: String,
}

/// The smallest HTTP client that can exercise this API.
fn request(port: u16, method: &str, path: &str, token: Option<&str>, body: Option<&str>) -> Reply {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect failed");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();

    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n");
    if let Some(token) = token {
        head.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if let Some(body) = body {
        head.push_str("Content-Type: application/json\r\n");
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).unwrap();
    if let Some(body) = body {
        stream.write_all(body.as_bytes()).unwrap();
    }

    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&raw).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    Reply { status, body }
}

fn start() -> (ApiServer, Arc<FakeBackend>, String) {
    let backend = Arc::new(FakeBackend::default());
    let token = String::from("test-token-12345");
    let server = ApiServer::start(backend.clone(), 0, token.clone()).expect("server did not start");
    (server, backend, token)
}

#[test]
fn health_is_open_but_everything_else_needs_the_token() {
    let (server, _backend, token) = start();
    let port = server.port();

    // Health carries no user data and answering it is how a script finds out
    // whether MasLight is even running.
    let health = request(port, "GET", "/api/health", None, None);
    assert_eq!(health.status, 200);
    assert!(health.body.contains("maslight"), "{}", health.body);

    let no_token = request(port, "GET", "/api/status", None, None);
    assert_eq!(no_token.status, 401, "status must not be readable");

    let wrong = request(port, "GET", "/api/status", Some("nope"), None);
    assert_eq!(wrong.status, 401);

    let right = request(port, "GET", "/api/status", Some(&token), None);
    assert_eq!(right.status, 200);
    assert!(right.body.contains("\"ledCount\":4"), "{}", right.body);
}

#[test]
fn a_token_in_the_query_string_works_for_browsers() {
    // A browser WebSocket cannot set headers, so the query string has to be
    // accepted or a page could never subscribe.
    let (server, _backend, token) = start();
    let reply = request(
        server.port(),
        "GET",
        &format!("/api/status?token={token}"),
        None,
        None,
    );
    assert_eq!(reply.status, 200);
}

#[test]
fn the_lights_can_be_switched_on_and_off() {
    let (server, backend, token) = start();
    let port = server.port();

    let reply = request(
        port,
        "POST",
        "/api/enabled",
        Some(&token),
        Some(r#"{"enabled":true}"#),
    );
    assert_eq!(reply.status, 200);
    assert!(backend.enabled.load(Ordering::SeqCst));
    assert!(reply.body.contains("\"enabled\":true"), "{}", reply.body);

    request(
        port,
        "POST",
        "/api/enabled",
        Some(&token),
        Some(r#"{"enabled":false}"#),
    );
    assert!(!backend.enabled.load(Ordering::SeqCst));
}

#[test]
fn a_profile_can_be_selected_and_a_missing_one_is_reported() {
    let (server, backend, token) = start();
    let port = server.port();

    let ok = request(
        port,
        "POST",
        "/api/profile",
        Some(&token),
        Some(r#"{"id":"cinema"}"#),
    );
    assert_eq!(ok.status, 200);
    assert_eq!(backend.profile.get(), "cinema");
    assert!(
        ok.body.contains("\"activeProfile\":\"cinema\""),
        "the reply should confirm what was committed: {}",
        ok.body
    );

    let missing = request(
        port,
        "POST",
        "/api/profile",
        Some(&token),
        Some(r#"{"id":"missing"}"#),
    );
    assert_eq!(missing.status, 404);
    assert!(missing.body.contains("error"), "{}", missing.body);
}

#[test]
fn a_colour_can_be_held_and_released() {
    let (server, backend, token) = start();
    let port = server.port();

    let held = request(
        port,
        "POST",
        "/api/hold",
        Some(&token),
        Some(r##"{"color":"#ff8800"}"##),
    );
    assert_eq!(held.status, 204);

    let released = request(
        port,
        "POST",
        "/api/hold",
        Some(&token),
        Some(r#"{"color":null}"#),
    );
    assert_eq!(released.status, 204);
    assert_eq!(backend.holds.load(Ordering::SeqCst), 2);

    let bad = request(
        port,
        "POST",
        "/api/hold",
        Some(&token),
        Some(r#"{"color":"not a colour"}"#),
    );
    assert_eq!(bad.status, 400, "a bad colour should be reported, not held");
    assert_eq!(
        backend.holds.load(Ordering::SeqCst),
        2,
        "and must not reach the engine"
    );
}

#[test]
fn a_single_led_can_be_flashed() {
    let (server, backend, token) = start();
    let reply = request(
        server.port(),
        "POST",
        "/api/identify",
        Some(&token),
        Some(r#"{"index":3}"#),
    );
    assert_eq!(reply.status, 204);
    assert_eq!(backend.identifies.load(Ordering::SeqCst), 1);
}

#[test]
fn the_configuration_round_trips() {
    let (server, _backend, token) = start();
    let port = server.port();

    let get = request(port, "GET", "/api/config", Some(&token), None);
    assert_eq!(get.status, 200);
    assert!(get.body.contains("\"profiles\""), "{}", get.body);

    let put = request(port, "PUT", "/api/config", Some(&token), Some(&get.body));
    assert_eq!(put.status, 200, "{}", put.body);
}

#[test]
fn a_malformed_body_is_a_bad_request_rather_than_a_crash() {
    let (server, _backend, token) = start();
    let reply = request(
        server.port(),
        "POST",
        "/api/enabled",
        Some(&token),
        Some("{not json"),
    );
    assert!(
        (400..500).contains(&reply.status),
        "expected a client error, got {}",
        reply.status
    );
}

#[test]
fn the_token_is_thirty_two_characters_and_changes() {
    let a = maslight_api::generate_token();
    let b = maslight_api::generate_token();
    assert_eq!(a.len(), 32);
    assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
    assert_ne!(a, b, "two tokens in a row must not be identical");
}

#[test]
fn tokens_do_not_repeat_and_do_not_lean_on_the_clock() {
    // What this checks is the shape of the output: no repeats, and every
    // position taking many different values. That is necessary but not
    // sufficient, and worth being clear about. The property that actually
    // matters is that a token cannot be predicted, and that comes from asking
    // the operating system for randomness rather than deriving it from the
    // clock and the process id. No unit test can demonstrate unpredictability;
    // it follows from where the bytes come from, which is why the source is
    // the thing to guard in review.
    let tokens: Vec<String> = (0..1000).map(|_| maslight_api::generate_token()).collect();

    let unique: std::collections::HashSet<&String> = tokens.iter().collect();
    assert_eq!(unique.len(), tokens.len(), "tokens repeated");

    for position in 0..32 {
        let seen: std::collections::HashSet<char> = tokens
            .iter()
            .filter_map(|t| t.chars().nth(position))
            .collect();
        assert!(
            seen.len() > 20,
            "position {position} only ever took {} values, which is not random",
            seen.len()
        );
    }
}

#[test]
fn a_hex_colour_parses_and_rubbish_does_not() {
    assert_eq!(
        maslight_api::parse_hex("#ff8800"),
        Some(Rgb8::new(255, 136, 0))
    );
    assert_eq!(
        maslight_api::parse_hex("00ff00"),
        Some(Rgb8::new(0, 255, 0))
    );
    assert_eq!(maslight_api::parse_hex("#fff"), None);
    assert_eq!(maslight_api::parse_hex("hello!"), None);
}
