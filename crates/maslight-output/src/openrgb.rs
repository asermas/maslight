//! The OpenRGB SDK protocol.
//!
//! OpenRGB drives motherboard headers, RAM, keyboards, fans and a long list of
//! other hardware that MasLight has no business talking to directly. Speaking
//! its SDK means one integration covers all of it.
//!
//! Every packet starts with the same sixteen byte header:
//!
//! ```text
//! 0..3   "ORGB"
//! 4..7   device index, little endian
//! 8..11  packet id, little endian
//! 12..15 payload length, little endian
//! ```
//!
//! The two packets that matter are `SetClientName`, sent once so a human can
//! see who is driving their lights, and `UpdateLeds`, sent per frame.

use maslight_core::{ColorOrder, LedFrame};

pub const PORT: u16 = 6742;
pub const MAGIC: [u8; 4] = *b"ORGB";

/// Packet ids from the OpenRGB SDK.
pub mod packet {
    pub const SET_CLIENT_NAME: u32 = 50;
    pub const UPDATE_LEDS: u32 = 1050;
    pub const SET_CUSTOM_MODE: u32 = 1100;
}

/// Build any packet from its id and payload.
pub fn frame_packet(device: u32, packet_id: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + payload.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&device.to_le_bytes());
    out.extend_from_slice(&packet_id.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// The handshake that names us in the OpenRGB interface.
pub fn set_client_name(name: &str) -> Vec<u8> {
    // The name is a C string, so it carries its terminator.
    let mut payload: Vec<u8> = name.bytes().take(64).collect();
    payload.push(0);
    frame_packet(0, packet::SET_CLIENT_NAME, &payload)
}

/// Put a device into its custom mode, which is what makes it accept colours.
pub fn set_custom_mode(device: u32) -> Vec<u8> {
    frame_packet(device, packet::SET_CUSTOM_MODE, &[])
}

/// One frame of colours for a device.
///
/// The payload is its own length, then the colour count, then four bytes per
/// colour: red, green, blue and a padding byte the protocol reserves.
pub fn update_leds(device: u32, frame: &LedFrame, order: ColorOrder) -> Vec<u8> {
    let count = frame.len().min(u16::MAX as usize);
    let payload_len = 4 + 2 + count * 4;
    let mut payload = Vec::with_capacity(payload_len);
    payload.extend_from_slice(&(payload_len as u32).to_le_bytes());
    payload.extend_from_slice(&(count as u16).to_le_bytes());
    for colour in frame.rgb.iter().take(count) {
        let [a, b, c] = colour.to_order(order);
        payload.extend_from_slice(&[a, b, c, 0]);
    }
    frame_packet(device, packet::UPDATE_LEDS, &payload)
}

/// A connection to an OpenRGB server.
mod sink {
    use std::io::Write;
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    use maslight_core::{ColorOrder, LedFrame};

    use crate::{OutputError, Sink, SinkCaps};

    pub struct OpenRgbSink {
        stream: TcpStream,
        host: String,
        device: u32,
        order: ColorOrder,
    }

    impl OpenRgbSink {
        pub fn connect(
            host: &str,
            port: u16,
            device: u32,
            order: ColorOrder,
        ) -> Result<Self, OutputError> {
            let addr: SocketAddr = crate::resolve(host, port)?;
            let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(1500))
                .map_err(|e| OutputError::Open(format!("{host}: {e}")))?;
            stream
                .set_nodelay(true)
                .map_err(|e| OutputError::Open(e.to_string()))?;
            stream
                .set_write_timeout(Some(Duration::from_millis(200)))
                .map_err(|e| OutputError::Open(e.to_string()))?;

            let mut sink = Self {
                stream,
                host: host.to_string(),
                device,
                order,
            };
            // Announce ourselves, then ask for the mode that accepts colours.
            // Both are one shot: a device left in an effect mode ignores
            // everything sent afterwards, which looks exactly like a dead
            // connection.
            sink.write(&super::set_client_name("MasLight"))?;
            sink.write(&super::set_custom_mode(device))?;
            Ok(sink)
        }

        fn write(&mut self, bytes: &[u8]) -> Result<(), OutputError> {
            self.stream
                .write_all(bytes)
                .map_err(|e| OutputError::Send(e.to_string()))
        }
    }

    impl Sink for OpenRgbSink {
        fn caps(&self) -> SinkCaps {
            SinkCaps {
                max_leds: u16::MAX as usize,
                rgbw: false,
                max_fps: 60,
            }
        }

        fn label(&self) -> String {
            format!("OpenRGB {} device {}", self.host, self.device)
        }

        fn push(&mut self, frame: &LedFrame) -> Result<(), OutputError> {
            let packet = super::update_leds(self.device, frame, self.order);
            self.write(&packet)
        }
    }
}

pub use sink::OpenRgbSink;
