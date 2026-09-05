//! E1.31 (sACN) data packets.
//!
//! One universe carries 512 DMX slots, so 170 RGB LEDs. Longer strips take
//! consecutive universes. The layout below is ANSI E1.31-2018 section 4.1.
//!
//! ```text
//! root layer    0..37    preamble, ACN id, flags+length, vector, CID
//! framing layer 38..114  flags+length, vector, source name, priority,
//!                        sync address, sequence, options, universe
//! DMP layer     115..    flags+length, vector, address+data type,
//!                        first address, increment, value count, start code
//! ```

use maslight_core::{ColorOrder, LedFrame};

pub const PORT: u16 = 5568;
/// Slots in one universe.
pub const SLOTS_PER_UNIVERSE: usize = 512;
/// RGB LEDs in one universe.
pub const LEDS_PER_UNIVERSE: usize = SLOTS_PER_UNIVERSE / 3;
/// Byte offset where DMX data begins.
pub const DATA_OFFSET: usize = 126;

const ACN_ID: [u8; 12] = [
    0x41, 0x53, 0x43, 0x2d, 0x45, 0x31, 0x2e, 0x31, 0x37, 0x00, 0x00, 0x00,
];
const VECTOR_ROOT_DATA: u32 = 0x0000_0004;
const VECTOR_FRAMING_DATA: u32 = 0x0000_0002;
const VECTOR_DMP_SET_PROPERTY: u8 = 0x02;

/// Component identifier. Constant so a receiver sees one stable sender, which
/// matters for sACN priority arbitration.
pub const MASLIGHT_CID: [u8; 16] = [
    0x6d, 0x61, 0x73, 0x6c, 0x69, 0x67, 0x68, 0x74, 0x00, 0x4d, 0x41, 0x53, 0x4c, 0x49, 0x47, 0x54,
];

/// Multicast group for a universe, per E1.31 section 9.3.1.
pub fn multicast_addr(universe: u16) -> std::net::Ipv4Addr {
    std::net::Ipv4Addr::new(239, 255, (universe >> 8) as u8, (universe & 0xff) as u8)
}

/// Build one packet per universe needed by the frame.
///
/// `sequence` wraps at 255 and must advance once per frame, per universe.
pub fn build_packets(
    frame: &LedFrame,
    order: ColorOrder,
    first_universe: u16,
    priority: u8,
    sequence: u8,
    source_name: &str,
) -> Vec<(u16, Vec<u8>)> {
    let mut dmx: Vec<u8> = Vec::with_capacity(frame.len() * 3);
    for c in &frame.rgb {
        dmx.extend_from_slice(&c.to_order(order));
    }

    let mut out = Vec::new();
    if dmx.is_empty() {
        return out;
    }
    for (i, chunk) in dmx.chunks(SLOTS_PER_UNIVERSE).enumerate() {
        let universe = first_universe.wrapping_add(i as u16);
        out.push((
            universe,
            build_universe(chunk, universe, priority, sequence, source_name),
        ));
    }
    out
}

/// Build a single universe packet from raw DMX slots.
pub fn build_universe(
    slots: &[u8],
    universe: u16,
    priority: u8,
    sequence: u8,
    source_name: &str,
) -> Vec<u8> {
    let n = slots.len().min(SLOTS_PER_UNIVERSE);
    let total = DATA_OFFSET + n;
    let mut p = Vec::with_capacity(total);

    // --- root layer ---
    p.extend_from_slice(&0x0010u16.to_be_bytes()); // preamble size
    p.extend_from_slice(&0x0000u16.to_be_bytes()); // post-amble size
    p.extend_from_slice(&ACN_ID);
    p.extend_from_slice(&flags_and_length(total - 16));
    p.extend_from_slice(&VECTOR_ROOT_DATA.to_be_bytes());
    p.extend_from_slice(&MASLIGHT_CID);

    // --- framing layer ---
    p.extend_from_slice(&flags_and_length(total - 38));
    p.extend_from_slice(&VECTOR_FRAMING_DATA.to_be_bytes());
    let mut name = [0u8; 64];
    for (slot, b) in name.iter_mut().zip(source_name.as_bytes().iter()).take(63) {
        *slot = *b;
    }
    p.extend_from_slice(&name);
    p.push(if priority == 0 { 100 } else { priority });
    p.extend_from_slice(&0u16.to_be_bytes()); // synchronization address
    p.push(sequence);
    p.push(0); // options
    p.extend_from_slice(&universe.to_be_bytes());

    // --- DMP layer ---
    p.extend_from_slice(&flags_and_length(total - 115));
    p.push(VECTOR_DMP_SET_PROPERTY);
    p.push(0xa1); // address type and data type
    p.extend_from_slice(&0x0000u16.to_be_bytes()); // first property address
    p.extend_from_slice(&0x0001u16.to_be_bytes()); // address increment
    p.extend_from_slice(&((n + 1) as u16).to_be_bytes()); // property value count
    p.push(0x00); // DMX start code
    p.extend_from_slice(&slots[..n]);

    p
}

/// Top nibble 0x7 plus a 12-bit PDU length.
#[inline]
fn flags_and_length(len: usize) -> [u8; 2] {
    let v = 0x7000u16 | (len as u16 & 0x0fff);
    v.to_be_bytes()
}
