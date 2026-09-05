//! WLED UDP realtime protocols.
//!
//! Byte 0 selects the protocol, byte 1 is how many seconds WLED waits after
//! the last packet before it goes back to its own effects.
//!
//! | id | name   | payload                                   | max LEDs/packet |
//! |----|--------|-------------------------------------------|-----------------|
//! | 1  | WARLS  | `index, r, g, b` per LED                  | 255 total       |
//! | 2  | DRGB   | `r, g, b` from LED 0                      | 490             |
//! | 3  | DRGBW  | `r, g, b, w` from LED 0                   | 367             |
//! | 4  | DNRGB  | 16-bit start index, then `r, g, b`        | 489 per packet  |
//! | 5  | DNRGBW | 16-bit start index, then `r, g, b, w`     | 367 per packet  |
//!
//! DNRGB is the default: it is the only one that scales past 490 LEDs, and the
//! extra two bytes per packet cost nothing.

use maslight_core::{ColorOrder, LedFrame, WledProtocol};

/// Largest UDP payload we will emit. WLED accepts 1472 bytes, which is what
/// fits in one 1500-byte Ethernet frame once IP and UDP headers are removed.
pub const MAX_PAYLOAD: usize = 1472;

/// Build the packets for one frame.
///
/// Returns one buffer per UDP datagram, already in wire order.
pub fn build_packets(
    protocol: WledProtocol,
    timeout_s: u8,
    frame: &LedFrame,
    order: ColorOrder,
) -> Vec<Vec<u8>> {
    // A caller that asks for an RGB protocol with an RGBW frame gets the white
    // channel folded back in rather than silently losing it.
    match protocol {
        WledProtocol::Warls => vec![warls(timeout_s, frame, order)],
        WledProtocol::Drgb => vec![drgb(timeout_s, frame, order, false)],
        WledProtocol::Drgbw => vec![drgb(timeout_s, frame, order, true)],
        WledProtocol::Dnrgb => dnrgb(timeout_s, frame, order, false),
        WledProtocol::Dnrgbw => dnrgb(timeout_s, frame, order, true),
    }
}

/// The protocol that can actually carry this many LEDs, used when the user
/// has not pinned one by hand.
pub fn protocol_for(len: usize, rgbw: bool) -> WledProtocol {
    match (len <= 490, rgbw) {
        (true, false) => WledProtocol::Drgb,
        (true, true) => WledProtocol::Drgbw,
        (false, false) => WledProtocol::Dnrgb,
        (false, true) => WledProtocol::Dnrgbw,
    }
}

fn white_of(frame: &LedFrame, i: usize) -> u8 {
    frame.white.get(i).copied().unwrap_or(0)
}

/// WARLS: `[1, timeout, (index, r, g, b) ...]`.
///
/// The index is a single byte, so WARLS cannot address past LED 255.
pub fn warls(timeout_s: u8, frame: &LedFrame, order: ColorOrder) -> Vec<u8> {
    let count = frame.len().min(255);
    let mut out = Vec::with_capacity(2 + count * 4);
    out.push(1);
    out.push(timeout_s);
    for (i, c) in frame.rgb.iter().take(count).enumerate() {
        out.push(i as u8);
        out.extend_from_slice(&c.to_order(order));
    }
    out
}

/// DRGB / DRGBW: `[2 or 3, timeout, r, g, b, (w) ...]` starting at LED 0.
pub fn drgb(timeout_s: u8, frame: &LedFrame, order: ColorOrder, rgbw: bool) -> Vec<u8> {
    let per_led = if rgbw { 4 } else { 3 };
    let max = (MAX_PAYLOAD - 2) / per_led;
    let count = frame.len().min(max);
    let mut out = Vec::with_capacity(2 + count * per_led);
    out.push(if rgbw { 3 } else { 2 });
    out.push(timeout_s);
    for (i, c) in frame.rgb.iter().take(count).enumerate() {
        out.extend_from_slice(&c.to_order(order));
        if rgbw {
            out.push(white_of(frame, i));
        }
    }
    out
}

/// DNRGB / DNRGBW: `[4 or 5, timeout, index_hi, index_lo, r, g, b, (w) ...]`,
/// split into as many datagrams as the strip needs.
pub fn dnrgb(timeout_s: u8, frame: &LedFrame, order: ColorOrder, rgbw: bool) -> Vec<Vec<u8>> {
    let per_led = if rgbw { 4 } else { 3 };
    let per_packet = (MAX_PAYLOAD - 4) / per_led;
    let total = frame.len();
    if total == 0 {
        return vec![vec![if rgbw { 5 } else { 4 }, timeout_s, 0, 0]];
    }

    let mut packets = Vec::new();
    let mut start = 0usize;
    while start < total {
        let end = (start + per_packet).min(total);
        let mut out = Vec::with_capacity(4 + (end - start) * per_led);
        out.push(if rgbw { 5 } else { 4 });
        out.push(timeout_s);
        out.push((start >> 8) as u8);
        out.push((start & 0xff) as u8);
        for i in start..end {
            out.extend_from_slice(&frame.rgb[i].to_order(order));
            if rgbw {
                out.push(white_of(frame, i));
            }
        }
        packets.push(out);
        start = end;
    }
    packets
}
