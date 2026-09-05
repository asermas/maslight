//! Art-Net 4 ArtDmx packets, UDP port 6454.
//!
//! ```text
//! 0..7   "Art-Net\0"
//! 8..9   OpCode 0x5000, little endian
//! 10..11 protocol version, high byte first, currently 14
//! 12     sequence, 1..255, 0 disables ordering
//! 13     physical input port, informational
//! 14     sub-universe: (subnet << 4) | universe
//! 15     net
//! 16..17 data length, high byte first, even, 2..512
//! 18..   DMX data
//! ```

use maslight_core::{ColorOrder, LedFrame};

pub const PORT: u16 = 6454;
pub const HEADER: [u8; 8] = [b'A', b'r', b't', b'-', b'N', b'e', b't', 0];
pub const OP_DMX: u16 = 0x5000;
pub const SLOTS_PER_UNIVERSE: usize = 512;
pub const LEDS_PER_UNIVERSE: usize = SLOTS_PER_UNIVERSE / 3;

/// Build one ArtDmx packet per universe the frame needs.
pub fn build_packets(
    frame: &LedFrame,
    order: ColorOrder,
    net: u8,
    subnet: u8,
    first_universe: u8,
    sequence: u8,
) -> Vec<Vec<u8>> {
    let mut dmx: Vec<u8> = Vec::with_capacity(frame.len() * 3);
    for c in &frame.rgb {
        dmx.extend_from_slice(&c.to_order(order));
    }
    if dmx.is_empty() {
        return Vec::new();
    }

    dmx.chunks(SLOTS_PER_UNIVERSE)
        .enumerate()
        .map(|(i, chunk)| {
            let universe = first_universe.wrapping_add(i as u8) & 0x0f;
            build_universe(chunk, net, subnet, universe, sequence)
        })
        .collect()
}

/// Build a single ArtDmx packet from raw DMX slots.
pub fn build_universe(slots: &[u8], net: u8, subnet: u8, universe: u8, sequence: u8) -> Vec<u8> {
    // Art-Net requires an even slot count of at least 2.
    let n = slots.len().min(SLOTS_PER_UNIVERSE);
    let padded = if n < 2 { 2 } else { n + (n & 1) };

    let mut p = Vec::with_capacity(18 + padded);
    p.extend_from_slice(&HEADER);
    p.extend_from_slice(&OP_DMX.to_le_bytes());
    p.push(0); // protocol version high
    p.push(14); // protocol version low
    p.push(sequence);
    p.push(0); // physical
    p.push(((subnet & 0x0f) << 4) | (universe & 0x0f));
    p.push(net & 0x7f);
    p.extend_from_slice(&(padded as u16).to_be_bytes());
    p.extend_from_slice(&slots[..n]);
    // Art-Net wants an even slot count, so pad the last channel with zero.
    p.extend(std::iter::repeat_n(0u8, padded - n));
    p
}
