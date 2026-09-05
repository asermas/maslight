//! Byte-for-byte wire format tests.
//!
//! These are the tests that matter most in this crate: a controller will
//! happily accept a malformed packet and show nothing, so the only way to know
//! the framing is right is to assert on the bytes.

use maslight_core::{ColorOrder, LedFrame, Rgb8, WledProtocol};
use maslight_output::{artnet, ddp, e131, serial, wled};

fn frame(colors: &[(u8, u8, u8)]) -> LedFrame {
    LedFrame {
        rgb: colors
            .iter()
            .map(|(r, g, b)| Rgb8::new(*r, *g, *b))
            .collect(),
        white: Vec::new(),
    }
}

fn rgbw_frame(colors: &[(u8, u8, u8, u8)]) -> LedFrame {
    LedFrame {
        rgb: colors
            .iter()
            .map(|(r, g, b, _)| Rgb8::new(*r, *g, *b))
            .collect(),
        white: colors.iter().map(|(_, _, _, w)| *w).collect(),
    }
}

// --- WLED ---------------------------------------------------------------

#[test]
fn warls_carries_an_index_per_led() {
    let f = frame(&[(10, 20, 30), (40, 50, 60)]);
    let p = wled::warls(2, &f, ColorOrder::Rgb);
    assert_eq!(p, vec![1, 2, 0, 10, 20, 30, 1, 40, 50, 60]);
}

#[test]
fn drgb_is_a_flat_stream_from_led_zero() {
    let f = frame(&[(1, 2, 3), (4, 5, 6)]);
    let p = wled::drgb(5, &f, ColorOrder::Rgb, false);
    assert_eq!(p, vec![2, 5, 1, 2, 3, 4, 5, 6]);
}

#[test]
fn drgbw_adds_the_white_channel() {
    let f = rgbw_frame(&[(1, 2, 3, 9)]);
    let p = wled::drgb(2, &f, ColorOrder::Rgb, true);
    assert_eq!(p, vec![3, 2, 1, 2, 3, 9]);
}

#[test]
fn colour_order_is_applied_on_the_wire_not_in_the_pipeline() {
    let f = frame(&[(10, 20, 30)]);
    let p = wled::drgb(2, &f, ColorOrder::Grb, false);
    // GRB means green first.
    assert_eq!(&p[2..5], &[20, 10, 30]);

    let p = wled::drgb(2, &f, ColorOrder::Bgr, false);
    assert_eq!(&p[2..5], &[30, 20, 10]);
}

#[test]
fn dnrgb_splits_long_strips_and_keeps_the_index_correct() {
    // 600 LEDs does not fit in one datagram: 489 in the first, 111 in the next.
    let f = frame(&vec![(1, 2, 3); 600]);
    let packets = wled::dnrgb(2, &f, ColorOrder::Rgb, false);
    assert_eq!(packets.len(), 2);

    assert_eq!(packets[0][0], 4, "protocol id");
    assert_eq!(&packets[0][2..4], &[0, 0], "first packet starts at LED 0");
    assert_eq!(packets[0].len(), 4 + 489 * 3);
    assert!(packets[0].len() <= wled::MAX_PAYLOAD);

    // 489 = 0x01E9
    assert_eq!(&packets[1][2..4], &[0x01, 0xE9]);
    assert_eq!(packets[1].len(), 4 + 111 * 3);
}

#[test]
fn dnrgb_of_a_short_strip_is_a_single_packet() {
    let f = frame(&[(255, 0, 0); 60]);
    let packets = wled::dnrgb(2, &f, ColorOrder::Grb, false);
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].len(), 4 + 60 * 3);
    // GRB: green byte first, so a pure red LED is 0, 255, 0.
    assert_eq!(&packets[0][4..7], &[0, 255, 0]);
}

#[test]
fn protocol_selection_matches_the_strip_length() {
    assert_eq!(wled::protocol_for(60, false), WledProtocol::Drgb);
    assert_eq!(wled::protocol_for(600, false), WledProtocol::Dnrgb);
    assert_eq!(wled::protocol_for(60, true), WledProtocol::Drgbw);
    assert_eq!(wled::protocol_for(600, true), WledProtocol::Dnrgbw);
}

#[test]
fn every_wled_packet_fits_in_one_ethernet_frame() {
    let f = frame(&vec![(255, 255, 255); 1500]);
    for protocol in [
        WledProtocol::Warls,
        WledProtocol::Drgb,
        WledProtocol::Drgbw,
        WledProtocol::Dnrgb,
        WledProtocol::Dnrgbw,
    ] {
        for p in wled::build_packets(protocol, 2, &f, ColorOrder::Grb) {
            assert!(
                p.len() <= wled::MAX_PAYLOAD,
                "{protocol:?} produced {} bytes",
                p.len()
            );
        }
    }
}

// --- DDP ----------------------------------------------------------------

#[test]
fn ddp_header_is_ten_bytes_with_push_on_the_last_packet() {
    let f = frame(&[(1, 2, 3), (4, 5, 6)]);
    let packets = ddp::build_packets(&f, ColorOrder::Rgb, 0, 1);
    assert_eq!(packets.len(), 1);
    let p = &packets[0];
    assert_eq!(p[0], ddp::FLAG_VER1 | ddp::FLAG_PUSH);
    assert_eq!(p[1], 1, "sequence");
    assert_eq!(p[2], ddp::TYPE_RGB8);
    assert_eq!(p[3], ddp::DEST_DISPLAY);
    assert_eq!(&p[4..8], &0u32.to_be_bytes(), "offset");
    assert_eq!(&p[8..10], &6u16.to_be_bytes(), "length");
    assert_eq!(&p[10..], &[1, 2, 3, 4, 5, 6]);
}

#[test]
fn ddp_splits_long_strips_and_only_pushes_once() {
    // 600 LEDs is 1800 bytes, which needs two 1440-byte datagrams.
    let f = frame(&vec![(9, 9, 9); 600]);
    let packets = ddp::build_packets(&f, ColorOrder::Rgb, 0, 3);
    assert_eq!(packets.len(), 2);
    assert_eq!(
        packets[0][0] & ddp::FLAG_PUSH,
        0,
        "only the last packet pushes"
    );
    assert_eq!(packets[1][0] & ddp::FLAG_PUSH, ddp::FLAG_PUSH);
    // The second packet continues at byte 1440.
    assert_eq!(&packets[1][4..8], &1440u32.to_be_bytes());
}

#[test]
fn ddp_marks_rgbw_data_with_its_own_type() {
    let f = rgbw_frame(&[(1, 2, 3, 4)]);
    let packets = ddp::build_packets(&f, ColorOrder::Rgb, 0, 1);
    assert_eq!(packets[0][2], ddp::TYPE_RGBW8);
    assert_eq!(&packets[0][10..], &[1, 2, 3, 4]);
}

// --- sACN ---------------------------------------------------------------

#[test]
fn e131_packet_has_the_e131_2018_layout() {
    let f = frame(&vec![(255, 128, 64); 170]);
    let packets = e131::build_packets(&f, ColorOrder::Rgb, 1, 100, 7, "MasLight");
    assert_eq!(packets.len(), 1);
    let (universe, p) = &packets[0];
    assert_eq!(*universe, 1);

    // 170 LEDs is 510 slots, so 126 + 510 bytes.
    assert_eq!(p.len(), 126 + 510);
    assert_eq!(&p[0..2], &[0x00, 0x10], "preamble size");
    assert_eq!(&p[4..16], b"ASC-E1.17\0\0\0", "ACN identifier");
    assert_eq!(&p[18..22], &4u32.to_be_bytes(), "root vector");
    assert_eq!(&p[40..44], &2u32.to_be_bytes(), "framing vector");
    assert_eq!(&p[44..52], b"MasLight", "source name");
    assert_eq!(p[108], 100, "priority");
    assert_eq!(p[111], 7, "sequence");
    assert_eq!(&p[113..115], &1u16.to_be_bytes(), "universe");
    assert_eq!(p[117], 0x02, "DMP vector");
    assert_eq!(p[118], 0xa1, "address and data type");
    assert_eq!(
        &p[123..125],
        &511u16.to_be_bytes(),
        "value count is slots + 1"
    );
    assert_eq!(p[125], 0x00, "DMX start code");
    assert_eq!(&p[126..129], &[255, 128, 64]);
}

#[test]
fn e131_pdu_lengths_agree_with_the_packet_size() {
    let f = frame(&vec![(1, 1, 1); 170]);
    let (_, p) = &e131::build_packets(&f, ColorOrder::Rgb, 1, 100, 0, "MasLight")[0];
    let total = p.len();
    let root = u16::from_be_bytes([p[16], p[17]]) & 0x0fff;
    let framing = u16::from_be_bytes([p[38], p[39]]) & 0x0fff;
    let dmp = u16::from_be_bytes([p[115], p[116]]) & 0x0fff;
    assert_eq!(root as usize, total - 16);
    assert_eq!(framing as usize, total - 38);
    assert_eq!(dmp as usize, total - 115);
    // Every PDU carries the 0x7 flags nibble.
    assert_eq!(p[16] & 0xf0, 0x70);
}

#[test]
fn e131_spills_into_consecutive_universes() {
    // 200 LEDs is 600 slots: universe 1 takes 512, universe 2 takes 88.
    let f = frame(&vec![(1, 2, 3); 200]);
    let packets = e131::build_packets(&f, ColorOrder::Rgb, 1, 100, 0, "MasLight");
    assert_eq!(packets.len(), 2);
    assert_eq!(packets[0].0, 1);
    assert_eq!(packets[1].0, 2);
    assert_eq!(packets[0].1.len(), 126 + 512);
    assert_eq!(packets[1].1.len(), 126 + 88);
}

#[test]
fn e131_multicast_group_follows_the_universe() {
    assert_eq!(
        e131::multicast_addr(1),
        std::net::Ipv4Addr::new(239, 255, 0, 1)
    );
    assert_eq!(
        e131::multicast_addr(258),
        std::net::Ipv4Addr::new(239, 255, 1, 2)
    );
}

// --- Art-Net ------------------------------------------------------------

#[test]
fn artnet_packet_has_the_artdmx_layout() {
    let f = frame(&[(7, 8, 9); 10]);
    let packets = artnet::build_packets(&f, ColorOrder::Rgb, 0, 0, 0, 4);
    assert_eq!(packets.len(), 1);
    let p = &packets[0];
    assert_eq!(&p[0..8], b"Art-Net\0");
    assert_eq!(&p[8..10], &[0x00, 0x50], "opcode is little endian");
    assert_eq!(&p[10..12], &[0, 14], "protocol version 14");
    assert_eq!(p[12], 4, "sequence");
    assert_eq!(p[14], 0, "sub-universe");
    assert_eq!(p[15], 0, "net");
    assert_eq!(&p[16..18], &30u16.to_be_bytes(), "length");
    assert_eq!(&p[18..21], &[7, 8, 9]);
}

#[test]
fn artnet_packs_net_subnet_and_universe_into_the_port_address() {
    let f = frame(&[(1, 1, 1)]);
    let p = &artnet::build_packets(&f, ColorOrder::Rgb, 2, 3, 5, 1)[0];
    assert_eq!(p[14], (3 << 4) | 5, "sub-universe is subnet and universe");
    assert_eq!(p[15], 2, "net");
}

#[test]
fn artnet_pads_odd_slot_counts() {
    // One LED is three slots, which is odd; Art-Net requires an even length.
    let f = frame(&[(1, 2, 3)]);
    let p = &artnet::build_packets(&f, ColorOrder::Rgb, 0, 0, 0, 1)[0];
    assert_eq!(&p[16..18], &4u16.to_be_bytes());
    assert_eq!(p.len(), 18 + 4);
    assert_eq!(p[21], 0, "padding byte is zero");
}

// --- serial -------------------------------------------------------------

#[test]
fn adalight_header_matches_the_reference_sketch() {
    let f = frame(&[(1, 2, 3); 60]);
    let p = serial::adalight_frame(&f, ColorOrder::Rgb);
    assert_eq!(&p[0..3], b"Ada");
    // count - 1 = 59
    assert_eq!(p[3], 0);
    assert_eq!(p[4], 59);
    assert_eq!(p[5], 59 ^ 0x55, "checksum: hi xor lo xor 0x55");
    assert_eq!(p.len(), 6 + 60 * 3);
}

#[test]
fn adalight_handles_a_long_strip_across_the_byte_boundary() {
    let f = frame(&vec![(0, 0, 0); 300]);
    let p = serial::adalight_frame(&f, ColorOrder::Rgb);
    // 299 = 0x012B
    assert_eq!(p[3], 0x01);
    assert_eq!(p[4], 0x2B);
    assert_eq!(p[5], 0x01 ^ 0x2B ^ 0x55);
}

#[test]
fn tpm2_frame_is_wrapped_in_its_block_markers() {
    let f = frame(&[(1, 2, 3), (4, 5, 6)]);
    let p = serial::tpm2_frame(&f, ColorOrder::Rgb);
    assert_eq!(p[0], 0xc9);
    assert_eq!(p[1], 0xda);
    assert_eq!(&p[2..4], &6u16.to_be_bytes());
    assert_eq!(&p[4..10], &[1, 2, 3, 4, 5, 6]);
    assert_eq!(*p.last().unwrap(), 0x36);
}

// --- sinks --------------------------------------------------------------

#[test]
fn identify_lights_exactly_one_led() {
    use maslight_output::{NullSink, Sink};
    let mut sink = NullSink::default();
    sink.identify(3, 8).unwrap();
    let f = sink.last.unwrap();
    assert_eq!(f.len(), 8);
    assert_eq!(f.rgb[3], Rgb8::new(255, 255, 255));
    assert!(f
        .rgb
        .iter()
        .enumerate()
        .all(|(i, c)| i == 3 || *c == Rgb8::BLACK));
}

#[test]
fn a_wled_sink_can_be_built_from_a_profile_device() {
    use maslight_core::DeviceConfig;
    let cfg = DeviceConfig::Wled {
        host: String::from("127.0.0.1"),
        port: 21324,
        protocol: WledProtocol::Dnrgb,
        timeout_s: 2,
    };
    let sink = maslight_output::make_sink(&cfg, ColorOrder::Grb).unwrap();
    assert_eq!(sink.label(), "WLED 127.0.0.1");
    assert_eq!(sink.caps().max_leds, 65_535);
}
