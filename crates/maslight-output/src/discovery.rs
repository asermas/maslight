//! Finding controllers on the local network so the user never types an IP.
//!
//! WLED advertises `_wled._tcp.local.` over mDNS. Once a host answers we ask
//! its JSON API how many LEDs it drives and what it is called, which is enough
//! to build a working device entry and a matching layout on the spot.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// A controller found on the network.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredDevice {
    /// Friendly name reported by the device, e.g. `wled-desk`.
    pub name: String,
    pub host: String,
    pub port: u16,
    /// LED count reported by the device, when it told us.
    pub led_count: Option<u32>,
    /// Firmware version string, when known.
    pub version: Option<String>,
    /// What kind of controller this is, currently always `wled`.
    pub kind: String,
}

/// Browse for WLED controllers for `timeout`.
///
/// Returns an empty list rather than an error when mDNS is unavailable, since
/// a firewall blocking multicast is a normal condition and the user can always
/// type an address by hand.
#[cfg(feature = "discovery")]
pub fn discover_wled(timeout: Duration) -> Vec<DiscoveredDevice> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};

    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!("mDNS unavailable: {e}");
            return Vec::new();
        }
    };
    let receiver = match daemon.browse("_wled._tcp.local.") {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("mDNS browse failed: {e}");
            return Vec::new();
        }
    };

    let deadline = Instant::now() + timeout;
    let mut found: BTreeMap<String, DiscoveredDevice> = BTreeMap::new();

    while Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(left) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let Some(addr) = info
                    .get_addresses()
                    .iter()
                    .find(|a| matches!(a, IpAddr::V4(_)))
                    .or_else(|| info.get_addresses().iter().next())
                    .copied()
                else {
                    continue;
                };
                let host = addr.to_string();
                let name = info
                    .get_fullname()
                    .split('.')
                    .next()
                    .unwrap_or("wled")
                    .to_string();
                found.insert(
                    host.clone(),
                    DiscoveredDevice {
                        name,
                        host,
                        port: info.get_port(),
                        led_count: None,
                        version: None,
                        kind: String::from("wled"),
                    },
                );
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.shutdown();

    // Ask each device about itself. Failures are not fatal.
    found
        .into_values()
        .map(|mut d| {
            if let Some(info) = query_wled_info(&d.host, Duration::from_millis(700)) {
                d.led_count = info.led_count;
                d.version = info.version;
                if let Some(n) = info.name {
                    d.name = n;
                }
            }
            d
        })
        .collect()
}

#[cfg(not(feature = "discovery"))]
pub fn discover_wled(_timeout: Duration) -> Vec<DiscoveredDevice> {
    Vec::new()
}

/// The subset of `/json/info` MasLight cares about.
#[derive(Clone, Debug, Default)]
pub struct WledInfo {
    pub name: Option<String>,
    pub led_count: Option<u32>,
    pub version: Option<String>,
}

/// Ask a WLED controller about itself over HTTP.
///
/// Hand-rolled rather than pulling in an HTTP client: this is one GET to a
/// device on the local network, and a dependency-free implementation keeps the
/// binary small and the audit surface tiny.
pub fn query_wled_info(host: &str, timeout: Duration) -> Option<WledInfo> {
    let body = http_get(host, 80, "/json/info", timeout)?;
    let value: serde_json::Value = serde_json::from_str(&body).ok()?;
    Some(WledInfo {
        name: value
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        led_count: value
            .get("leds")
            .and_then(|l| l.get("count"))
            .and_then(|c| c.as_u64())
            .map(|c| c as u32),
        version: value
            .get("ver")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

/// Minimal HTTP/1.1 GET that returns the response body.
fn http_get(host: &str, port: u16, path: &str, timeout: Duration) -> Option<String> {
    let addr: SocketAddr = match host.parse::<IpAddr>() {
        Ok(ip) => SocketAddr::new(ip, port),
        Err(_) => {
            use std::net::ToSocketAddrs;
            (host, port).to_socket_addrs().ok()?.next()?
        }
    };
    let mut stream = TcpStream::connect_timeout(&addr, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: MasLight\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).ok()?;

    let mut raw = Vec::new();
    let mut buf = [0u8; 2048];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                // A controller answer is a few hundred bytes; anything much
                // larger is not something we should be parsing.
                if raw.len() > 64 * 1024 {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let text = String::from_utf8_lossy(&raw);
    let body = text.split("\r\n\r\n").nth(1)?.to_string();
    // WLED answers without chunked encoding, but be forgiving about a
    // trailing chunk marker if some firmware adds one.
    Some(body.trim_end_matches("\r\n0\r\n\r\n").to_string())
}
