//! Publishing state to an MQTT broker.
//!
//! Not a frame stream. Sixty LEDs sixty times a second is a thousand messages
//! and four megabytes a minute, which is not what a broker is for and not what
//! anyone subscribing actually wants.
//!
//! What home automation wants is state: is it on, which profile, roughly what
//! colour. So this publishes a small JSON object a couple of times a second,
//! and Home Assistant or Node-RED can act on it.
//!
//! The client is written out rather than pulled in. Publishing at quality of
//! service zero is a CONNECT packet and a PUBLISH packet, both a few dozen
//! lines, and an MQTT library would be a large dependency for two packet
//! shapes.

use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use maslight_core::{LedFrame, Rgb8};

use crate::{OutputError, Sink, SinkCaps};

pub const PORT: u16 = 1883;

/// How often state is published.
const INTERVAL: Duration = Duration::from_millis(500);

/// MQTT 3.1.1 CONNECT with a client id and a clean session.
pub fn connect_packet(client_id: &str) -> Vec<u8> {
    let mut payload = Vec::new();
    push_string(&mut payload, "MQTT");
    payload.push(0x04); // protocol level 4, which is 3.1.1
    payload.push(0x02); // clean session, no will, no credentials
    payload.extend_from_slice(&60u16.to_be_bytes()); // keep alive, seconds
    push_string(&mut payload, client_id);

    let mut out = vec![0x10]; // CONNECT
    push_remaining_length(&mut out, payload.len());
    out.extend_from_slice(&payload);
    out
}

/// PUBLISH at quality of service zero.
pub fn publish_packet(topic: &str, payload: &[u8], retain: bool) -> Vec<u8> {
    let mut body = Vec::new();
    push_string(&mut body, topic);
    body.extend_from_slice(payload);

    // Retain matters here: a subscriber that connects later should learn the
    // current state rather than wait for the next tick.
    let mut out = vec![if retain { 0x31 } else { 0x30 }];
    push_remaining_length(&mut out, body.len());
    out.extend_from_slice(&body);
    out
}

/// A PINGREQ, so a quiet connection is not dropped by the broker.
pub fn ping_packet() -> Vec<u8> {
    vec![0xc0, 0x00]
}

fn push_string(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    let len = bytes.len().min(u16::MAX as usize);
    out.extend_from_slice(&(len as u16).to_be_bytes());
    out.extend_from_slice(&bytes[..len]);
}

/// MQTT encodes lengths seven bits at a time, high bit as a continuation flag.
fn push_remaining_length(out: &mut Vec<u8>, mut len: usize) {
    loop {
        let mut byte = (len % 128) as u8;
        len /= 128;
        if len > 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if len == 0 {
            break;
        }
    }
}

/// The state MasLight publishes.
pub fn state_json(frame: &LedFrame) -> String {
    let average = average(frame);
    let on = frame.rgb.iter().any(|c| c.r > 0 || c.g > 0 || c.b > 0);
    format!(
        "{{\"state\":\"{}\",\"leds\":{},\"color\":{{\"r\":{},\"g\":{},\"b\":{}}},\"hex\":\"#{:02x}{:02x}{:02x}\"}}",
        if on { "ON" } else { "OFF" },
        frame.len(),
        average.r,
        average.g,
        average.b,
        average.r,
        average.g,
        average.b
    )
}

/// Mean colour of a frame.
///
/// Averaged in the 8-bit domain on purpose: this is a summary for a dashboard,
/// not a value anything renders, and matching what a person would call the
/// average of the numbers they can see is more useful here than being
/// photometrically correct.
fn average(frame: &LedFrame) -> Rgb8 {
    if frame.is_empty() {
        return Rgb8::BLACK;
    }
    let mut sum = [0u32; 3];
    for c in &frame.rgb {
        sum[0] += c.r as u32;
        sum[1] += c.g as u32;
        sum[2] += c.b as u32;
    }
    let n = frame.len() as u32;
    Rgb8::new((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8)
}

/// Publishes MasLight state to a broker.
pub struct MqttSink {
    stream: TcpStream,
    host: String,
    topic: String,
    last: Instant,
    last_payload: String,
}

impl MqttSink {
    pub fn connect(host: &str, port: u16, topic: &str) -> Result<Self, OutputError> {
        let addr: SocketAddr = crate::resolve(host, port)?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(1500))
            .map_err(|e| OutputError::Open(format!("{host}: {e}")))?;
        stream
            .set_write_timeout(Some(Duration::from_millis(300)))
            .map_err(|e| OutputError::Open(e.to_string()))?;
        stream
            .write_all(&connect_packet("maslight"))
            .map_err(|e| OutputError::Open(e.to_string()))?;

        Ok(Self {
            stream,
            host: host.to_string(),
            topic: topic.to_string(),
            // Publish the first frame immediately rather than after a wait.
            last: Instant::now() - INTERVAL,
            last_payload: String::new(),
        })
    }
}

impl Sink for MqttSink {
    fn caps(&self) -> SinkCaps {
        SinkCaps {
            max_leds: usize::MAX,
            rgbw: false,
            // The rate limit lives inside push, so this is what actually
            // leaves the machine rather than what arrives.
            max_fps: 2,
        }
    }

    fn label(&self) -> String {
        format!("MQTT {} {}", self.host, self.topic)
    }

    fn push(&mut self, frame: &LedFrame) -> Result<(), OutputError> {
        if self.last.elapsed() < INTERVAL {
            return Ok(());
        }
        let payload = state_json(frame);
        // A broker does not need to be told the same thing twice, and a
        // subscriber acting on every message should not be woken for nothing.
        if payload == self.last_payload {
            self.last = Instant::now();
            return self
                .stream
                .write_all(&ping_packet())
                .map_err(|e| OutputError::Send(e.to_string()));
        }
        self.last = Instant::now();
        self.last_payload = payload.clone();
        self.stream
            .write_all(&publish_packet(&self.topic, payload.as_bytes(), true))
            .map_err(|e| OutputError::Send(e.to_string()))
    }
}
