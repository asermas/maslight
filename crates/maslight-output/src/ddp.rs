//! Distributed Display Protocol (3waylabs DDP), UDP port 4048.
//!
//! Header, 10 bytes:
//!
//! ```text
//! 0  flags     0x40 version 1, | 0x01 PUSH on the last packet of a frame
//! 1  sequence  1..15, 0 means unused
//! 2  type      C R TTT SSS -> 0x0B is RGB with 8 bits per channel
//! 3  dest id   1 is the default output device
//! 4..7  offset in bytes, big endian
//! 8..9  data length in bytes, big endian
//! ```
//!
//! DDP has no per-universe limit, so a long strip only costs more datagrams.

use maslight_core::{ColorOrder, LedFrame};

/// Payload bytes per datagram. 1440 is 480 RGB pixels and is what most
/// receivers, WLED included, are tested against.
pub const MAX_DATA: usize = 1440;

pub const FLAG_VER1: u8 = 0x40;
pub const FLAG_PUSH: u8 = 0x01;
/// RGB, 8 bits per channel: type bits 001, size bits 011.
pub const TYPE_RGB8: u8 = 0x0B;
/// RGBW, 8 bits per channel: type bits 011, size bits 011.
pub const TYPE_RGBW8: u8 = 0x1B;
pub const DEST_DISPLAY: u8 = 1;

/// Build the datagrams for one frame.
///
/// `sequence` is advanced by the caller and wraps inside 1..=15.
pub fn build_packets(
    frame: &LedFrame,
    order: ColorOrder,
    start_offset: u32,
    sequence: u8,
) -> Vec<Vec<u8>> {
    let rgbw = frame.is_rgbw();
    let per_led = if rgbw { 4 } else { 3 };
    let leds_per_packet = MAX_DATA / per_led;
    let total = frame.len();

    let mut packets = Vec::new();
    let mut start = 0usize;
    let seq = if sequence == 0 {
        0
    } else {
        ((sequence - 1) % 15) + 1
    };

    loop {
        let end = (start + leds_per_packet).min(total);
        let is_last = end >= total;
        let data_len = (end - start) * per_led;

        let mut out = Vec::with_capacity(10 + data_len);
        out.push(FLAG_VER1 | if is_last { FLAG_PUSH } else { 0 });
        out.push(seq);
        out.push(if rgbw { TYPE_RGBW8 } else { TYPE_RGB8 });
        out.push(DEST_DISPLAY);
        let offset = start_offset as usize + start * per_led;
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(data_len as u16).to_be_bytes());
        for i in start..end {
            out.extend_from_slice(&frame.rgb[i].to_order(order));
            if rgbw {
                out.push(frame.white.get(i).copied().unwrap_or(0));
            }
        }
        packets.push(out);

        if is_last {
            break;
        }
        start = end;
    }
    packets
}
