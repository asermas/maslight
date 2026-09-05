//! OpenRGB and MQTT, against fake servers on a real socket.
//!
//! Both are stream protocols where the shape of the bytes decides whether
//! anything happens at all, so the tests read what actually arrived rather
//! than trusting a builder in isolation.

use std::io::Read;
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use maslight_core::{ColorOrder, DeviceConfig, LedFrame, Rgb8};
use maslight_output::{make_sink, mqtt, openrgb, Sink};

fn frame(colors: &[(u8, u8, u8)]) -> LedFrame {
    LedFrame {
        rgb: colors
            .iter()
            .map(|(r, g, b)| Rgb8::new(*r, *g, *b))
            .collect(),
        white: Vec::new(),
    }
}

/// A server that accepts one connection and forwards everything it reads.
fn fake_server() -> (u16, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind failed");
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
        let mut buf = [0u8; 8192];
        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                break;
            }
            if tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    (port, rx)
}

/// Collect bytes until `want` of them have arrived or the wait runs out.
fn collect(rx: &mpsc::Receiver<Vec<u8>>, want: usize) -> Vec<u8> {
    let mut all = Vec::new();
    while all.len() < want {
        match rx.recv_timeout(Duration::from_secs(3)) {
            Ok(chunk) => all.extend_from_slice(&chunk),
            Err(_) => break,
        }
    }
    all
}

// --- OpenRGB -------------------------------------------------------------

#[test]
fn an_openrgb_packet_has_the_sdk_header() {
    let packet = openrgb::frame_packet(2, openrgb::packet::UPDATE_LEDS, &[1, 2, 3]);
    assert_eq!(&packet[0..4], b"ORGB");
    assert_eq!(&packet[4..8], &2u32.to_le_bytes(), "device index");
    assert_eq!(&packet[8..12], &1050u32.to_le_bytes(), "packet id");
    assert_eq!(&packet[12..16], &3u32.to_le_bytes(), "payload length");
    assert_eq!(&packet[16..], &[1, 2, 3]);
}

#[test]
fn openrgb_colours_carry_a_padding_byte() {
    // The protocol reserves a fourth byte per colour. Getting the stride wrong
    // shifts every LED, which looks like a working connection showing rubbish.
    let f = frame(&[(10, 20, 30), (40, 50, 60)]);
    let packet = openrgb::update_leds(0, &f, ColorOrder::Rgb);
    let payload = &packet[16..];

    assert_eq!(
        &payload[0..4],
        &(4u32 + 2 + 8).to_le_bytes(),
        "payload length"
    );
    assert_eq!(&payload[4..6], &2u16.to_le_bytes(), "colour count");
    assert_eq!(&payload[6..14], &[10, 20, 30, 0, 40, 50, 60, 0]);
}

#[test]
fn the_client_name_is_a_terminated_string() {
    let packet = openrgb::set_client_name("MasLight");
    assert_eq!(&packet[16..24], b"MasLight");
    assert_eq!(packet[24], 0, "the name is a C string");
}

#[test]
fn openrgb_announces_itself_and_asks_for_custom_mode_before_any_colour() {
    // A device left in an effect mode ignores colours, which looks exactly
    // like a dead connection, so the handshake is not optional.
    let (port, rx) = fake_server();
    let mut sink = openrgb::OpenRgbSink::connect("127.0.0.1", port, 3, ColorOrder::Grb)
        .expect("should connect");

    let handshake = collect(&rx, 16 + 9 + 16);
    assert_eq!(&handshake[0..4], b"ORGB");
    assert_eq!(
        &handshake[8..12],
        &openrgb::packet::SET_CLIENT_NAME.to_le_bytes(),
        "the first packet should name us"
    );

    let second = &handshake[16 + 9..];
    assert_eq!(
        &second[8..12],
        &openrgb::packet::SET_CUSTOM_MODE.to_le_bytes(),
        "then ask for the mode that accepts colours"
    );
    assert_eq!(&second[4..8], &3u32.to_le_bytes(), "on the chosen device");

    sink.push(&frame(&[(255, 0, 0)])).expect("push failed");
    let update = collect(&rx, 16 + 4 + 2 + 4);
    assert_eq!(&update[8..12], &openrgb::packet::UPDATE_LEDS.to_le_bytes());
    // GRB on the wire, so a pure red LED leaves as 0, 255, 0.
    assert_eq!(&update[22..26], &[0, 255, 0, 0]);

    assert_eq!(sink.label(), "OpenRGB 127.0.0.1 device 3");
}

#[test]
fn an_openrgb_server_that_is_not_there_is_an_error_not_a_panic() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let result = openrgb::OpenRgbSink::connect("127.0.0.1", port, 0, ColorOrder::Rgb);
    assert!(result.is_err());
}

// --- MQTT ----------------------------------------------------------------

#[test]
fn the_connect_packet_is_mqtt_3_1_1() {
    let packet = mqtt::connect_packet("maslight");
    assert_eq!(packet[0], 0x10, "CONNECT");
    // Remaining length, then the protocol name as a length prefixed string.
    assert_eq!(&packet[2..4], &4u16.to_be_bytes());
    assert_eq!(&packet[4..8], b"MQTT");
    assert_eq!(packet[8], 4, "protocol level 4 is 3.1.1");
    assert_eq!(packet[9], 0x02, "clean session");
    assert_eq!(&packet[10..12], &60u16.to_be_bytes(), "keep alive");
    assert_eq!(&packet[14..22], b"maslight", "client id");
}

#[test]
fn a_publish_carries_the_topic_and_is_retained() {
    let packet = mqtt::publish_packet("maslight/state", b"hello", true);
    assert_eq!(packet[0], 0x31, "PUBLISH with the retain flag");
    assert_eq!(&packet[2..4], &14u16.to_be_bytes(), "topic length");
    assert_eq!(&packet[4..18], b"maslight/state");
    assert_eq!(&packet[18..], b"hello");

    let plain = mqtt::publish_packet("t", b"x", false);
    assert_eq!(plain[0], 0x30, "without retain");
}

#[test]
fn long_payloads_use_the_multi_byte_length() {
    // MQTT encodes lengths seven bits at a time. A payload over 127 bytes is
    // where a naive single byte length silently truncates.
    let payload = vec![b'x'; 300];
    let packet = mqtt::publish_packet("t", &payload, false);
    assert_eq!(packet[0], 0x30);
    // 3 for the topic, 300 for the payload, so 303 = 0xAF 0x02.
    assert_eq!(packet[1], 0xaf);
    assert_eq!(packet[2], 0x02);
    assert_eq!(packet.len(), 3 + 303);
}

#[test]
fn the_state_summary_says_what_a_dashboard_needs() {
    let lit = mqtt::state_json(&frame(&[(255, 0, 0), (0, 0, 255)]));
    assert!(lit.contains("\"state\":\"ON\""), "{lit}");
    assert!(lit.contains("\"leds\":2"), "{lit}");
    // The average of pure red and pure blue.
    assert!(lit.contains("\"r\":127"), "{lit}");
    assert!(lit.contains("\"b\":127"), "{lit}");
    assert!(lit.contains("\"hex\":\"#7f007f\""), "{lit}");

    let dark = mqtt::state_json(&frame(&[(0, 0, 0), (0, 0, 0)]));
    assert!(dark.contains("\"state\":\"OFF\""), "{dark}");
}

#[test]
fn mqtt_publishes_state_rather_than_streaming_frames() {
    // Sixty LEDs sixty times a second is not what a broker is for. The sink
    // rate limits, and repeats become a ping rather than another publish.
    let (port, rx) = fake_server();
    let mut sink =
        mqtt::MqttSink::connect("127.0.0.1", port, "maslight/state").expect("should connect");

    let red = frame(&[(255, 0, 0); 8]);
    for _ in 0..30 {
        sink.push(&red).expect("push failed");
    }

    // Read the whole conversation rather than a fixed number of bytes: the
    // point of the test is how many packets there are, not how long they are.
    let stream = drain(&rx);
    assert_eq!(stream[0], 0x10, "the connection opens with CONNECT");

    let publishes = stream.iter().filter(|b| **b == 0x31).count();
    let text = String::from_utf8_lossy(&stream);
    assert!(text.contains("maslight/state"), "{text}");
    assert!(text.contains("\"state\":\"ON\""), "{text}");
    assert!(
        publishes <= 2,
        "thirty frames should not be thirty publishes, saw {publishes}"
    );

    assert_eq!(
        sink.caps().max_fps,
        2,
        "the rate limit is part of the contract"
    );
}

// --- the factory ---------------------------------------------------------

#[test]
fn both_can_be_built_from_a_profile_device() {
    let (openrgb_port, _rx) = fake_server();
    let sink = make_sink(
        &DeviceConfig::OpenRgb {
            host: String::from("127.0.0.1"),
            port: openrgb_port,
            device: 1,
        },
        ColorOrder::Rgb,
    )
    .expect("openrgb sink");
    assert!(sink.label().starts_with("OpenRGB"));

    let (mqtt_port, _rx2) = fake_server();
    let sink = make_sink(
        &DeviceConfig::Mqtt {
            host: String::from("127.0.0.1"),
            port: mqtt_port,
            topic: String::from("home/leds"),
        },
        ColorOrder::Rgb,
    )
    .expect("mqtt sink");
    assert_eq!(sink.label(), "MQTT 127.0.0.1 home/leds");
}

/// Read everything the server received, stopping once it goes quiet.
fn drain(rx: &mpsc::Receiver<Vec<u8>>) -> Vec<u8> {
    let mut all = Vec::new();
    while let Ok(chunk) = rx.recv_timeout(Duration::from_millis(500)) {
        all.extend_from_slice(&chunk);
    }
    all
}
