//! Point a real controller at a known pattern.
//!
//! This is the first thing to run when a strip stays dark. It talks to the
//! controller directly, with no capture and no colour pipeline in the way, so
//! whatever happens tells you which half of the system to look at.
//!
//! ```text
//! cargo run -p maslight-output --example wledtest -- 192.168.0.200 64
//! cargo run -p maslight-output --example wledtest -- 192.168.0.200 64 grb
//! ```
//!
//! In order it sends: red, green, blue, white, a running dot, and then black.
//! If red comes out green, the colour order is wrong and the third argument is
//! how you try another one.

use std::thread::sleep;
use std::time::{Duration, Instant};

use maslight_core::{ColorOrder, LedFrame, Rgb8, WledProtocol};
use maslight_output::{make_sink, Sink};

fn main() {
    let mut args = std::env::args().skip(1);
    let host = args.next().unwrap_or_else(|| {
        eprintln!("usage: wledtest <host> [led count] [rgb|grb|bgr|rbg|gbr|brg]");
        std::process::exit(2);
    });
    let count: usize = args
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60)
        .clamp(1, 4096);
    let order = match args.next().as_deref() {
        Some("rgb") => ColorOrder::Rgb,
        Some("rbg") => ColorOrder::Rbg,
        Some("gbr") => ColorOrder::Gbr,
        Some("brg") => ColorOrder::Brg,
        Some("bgr") => ColorOrder::Bgr,
        _ => ColorOrder::Grb,
    };

    println!("asking {host} about itself");
    match maslight_output::discovery::query_wled_info(&host, Duration::from_millis(1500)) {
        Some(info) => println!(
            "  name: {}\n  leds: {}\n  firmware: {}",
            info.name.unwrap_or_else(|| String::from("unknown")),
            info.led_count
                .map(|c| c.to_string())
                .unwrap_or_else(|| String::from("unknown")),
            info.version.unwrap_or_else(|| String::from("unknown"))
        ),
        None => println!("  no answer on port 80, carrying on with UDP anyway"),
    }

    let config = maslight_core::DeviceConfig::Wled {
        host: host.clone(),
        port: 21324,
        protocol: if count > 490 {
            WledProtocol::Dnrgb
        } else {
            WledProtocol::Drgb
        },
        timeout_s: 2,
    };
    let mut sink = match make_sink(&config, order) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot open the output: {e}");
            std::process::exit(1);
        }
    };
    println!("sending {count} LEDs to {host} as {}", order.as_str());

    let solid = |name: &str, colour: Rgb8, sink: &mut Box<dyn Sink>| {
        println!("  {name}");
        let frame = LedFrame {
            rgb: vec![colour; count],
            white: Vec::new(),
        };
        // WLED drops back to its own effects after the hold time, so keep
        // resending while the colour is meant to stay up.
        let until = Instant::now() + Duration::from_millis(1500);
        while Instant::now() < until {
            if let Err(e) = sink.push(&frame) {
                eprintln!("send failed: {e}");
                return;
            }
            sleep(Duration::from_millis(40));
        }
    };

    solid("red", Rgb8::new(255, 0, 0), &mut sink);
    solid("green", Rgb8::new(0, 255, 0), &mut sink);
    solid("blue", Rgb8::new(0, 0, 255), &mut sink);
    solid("white", Rgb8::new(255, 255, 255), &mut sink);

    println!("  running dot, watch which end it starts from");
    for i in 0..count {
        let mut frame = LedFrame::black(count);
        frame.rgb[i] = Rgb8::new(255, 255, 255);
        if let Err(e) = sink.push(&frame) {
            eprintln!("send failed: {e}");
            break;
        }
        sleep(Duration::from_millis(30));
    }

    println!("  off");
    let _ = sink.blackout(count);
    println!("done");
}
