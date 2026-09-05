//! End-to-end tests: synthetic screen in, real UDP packets out.
//!
//! These run the actual engine thread, so they cover the parts that unit tests
//! cannot: rebuilding on configuration changes, the latency delay line, sink
//! failures, and the shape of what finally lands on the wire.

use std::net::UdpSocket;
use std::time::{Duration, Instant};

use maslight_core::{
    AppConfig, CaptureBackendKind, ColorSettings, DeviceConfig, EdgeCounts, Layout, LightMode,
    Profile, WizardParams, WledProtocol,
};
use maslight_engine::EngineHandle;

/// A four-LED layout on the synthetic display: one probe per screen edge.
fn test_profile(devices: Vec<DeviceConfig>) -> Profile {
    let layout = Layout::from_wizard(&WizardParams {
        display: String::from("test"),
        counts: EdgeCounts {
            top: 1,
            right: 1,
            bottom: 1,
            left: 1,
        },
        depth: 0.2,
        ..Default::default()
    });
    Profile {
        id: String::from("test"),
        name: String::from("Test"),
        mode: LightMode::Screen,
        layout,
        color: ColorSettings {
            // No smoothing or dithering: the first frame is the final frame.
            smoothing_ms: 0.0,
            dithering: false,
            ..Default::default()
        },
        capture: maslight_core::CaptureSettings {
            backend: CaptureBackendKind::Test,
            target_fps: 60,
            adaptive: false,
            ..Default::default()
        },
        devices,
        ..Default::default()
    }
}

fn config_with(profile: Profile) -> AppConfig {
    AppConfig {
        active_profile: profile.id.clone(),
        profiles: vec![profile],
        enabled: true,
        ..Default::default()
    }
}

/// Poll until `f` is happy or the deadline passes.
fn wait_for<T>(timeout: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(v) = f() {
            return Some(v);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

/// Serialises the tests in this file.
///
/// One of them makes the synthetic capture backend refuse to start, and that
/// switch is process-wide: the engine owns its backend on its own thread and
/// there is no seam to hand one in. Cargo runs tests in a binary in parallel
/// by default, so without this another test would consume the injected
/// failures and fail for a reason that has nothing to do with what it checks.
///
/// A poisoned lock is not interesting here: a panic in one test should not
/// turn every later one into a second failure that hides it.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn the_engine_lights_leds_from_the_synthetic_screen() {
    let _serial = serial();
    let engine = EngineHandle::spawn(config_with(test_profile(vec![DeviceConfig::Null])));

    let status = wait_for(Duration::from_secs(5), || {
        let s = engine.status();
        (s.leds.len() == 4 && s.leds.iter().any(|c| c.r > 0 || c.g > 0 || c.b > 0)).then_some(s)
    })
    .expect("engine never produced a lit frame");

    assert!(status.running);
    assert_eq!(status.led_count, 4);
    assert_eq!(status.capture_backend, "Test");
    assert!(status.frames > 0);

    // The synthetic pattern is red top-left, green top-right, blue
    // bottom-left, white bottom-right. The bottom edge probe therefore sees
    // blue and white mixed, so it must carry a strong blue.
    let bottom = status.leds[0];
    assert!(
        bottom.b > 100,
        "bottom probe should be blue-heavy, got {bottom:?}"
    );
}

#[test]
fn frames_reach_the_wire_as_valid_wled_packets() {
    let _serial = serial();
    // Bind first so the port is known and nothing is missed.
    let listener = UdpSocket::bind("127.0.0.1:0").expect("bind failed");
    listener
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let port = listener.local_addr().unwrap().port();

    let engine = EngineHandle::spawn(config_with(test_profile(vec![DeviceConfig::Wled {
        host: String::from("127.0.0.1"),
        port,
        protocol: WledProtocol::Dnrgb,
        timeout_s: 2,
    }])));

    let mut buf = [0u8; 2048];
    let (n, _) = listener.recv_from(&mut buf).expect("no packet arrived");
    let packet = &buf[..n];

    // DNRGB: protocol 4, timeout 2, start index 0, then three bytes per LED.
    assert_eq!(packet[0], 4, "protocol id");
    assert_eq!(packet[1], 2, "timeout");
    assert_eq!(&packet[2..4], &[0, 0], "start index");
    assert_eq!(packet.len(), 4 + 4 * 3, "four LEDs of RGB data");

    // Something has to be lit: an all-zero payload means the pipeline stalled.
    assert!(
        packet[4..].iter().any(|b| *b > 0),
        "payload was entirely black"
    );

    drop(engine);
}

#[test]
fn disabling_the_engine_blacks_the_strip_out() {
    let _serial = serial();
    let listener = UdpSocket::bind("127.0.0.1:0").expect("bind failed");
    listener
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let port = listener.local_addr().unwrap().port();

    let engine = EngineHandle::spawn(config_with(test_profile(vec![DeviceConfig::Wled {
        host: String::from("127.0.0.1"),
        port,
        protocol: WledProtocol::Dnrgb,
        timeout_s: 2,
    }])));

    // Wait until it is definitely running.
    wait_for(Duration::from_secs(5), || {
        engine
            .status()
            .leds
            .iter()
            .any(|c| c.r > 0 || c.b > 0)
            .then_some(())
    })
    .expect("engine never started");

    engine.set_enabled(false);

    // Every packet from here on must be black. Allow a few in-flight frames.
    let mut buf = [0u8; 2048];
    let mut black_run = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && black_run < 3 {
        match listener.recv_from(&mut buf) {
            Ok((n, _)) => {
                if buf[4..n].iter().all(|b| *b == 0) {
                    black_run += 1;
                } else {
                    black_run = 0;
                }
            }
            Err(_) => break,
        }
    }
    assert!(black_run >= 3, "strip never went dark after being disabled");
    assert!(!engine.status().enabled);
}

#[test]
fn identify_drives_a_single_led_for_the_calibration_wizard() {
    let _serial = serial();
    let engine = EngineHandle::spawn(config_with(test_profile(vec![DeviceConfig::Null])));
    wait_for(Duration::from_secs(5), || {
        (engine.status().leds.len() == 4).then_some(())
    })
    .expect("engine never started");

    engine.identify(2, 1500);

    let leds = wait_for(Duration::from_secs(3), || {
        let s = engine.status();
        (s.leds.len() == 4 && s.leds[2].r == 255 && s.leds[2].g == 255).then_some(s.leds)
    })
    .expect("identify never lit the requested LED");

    assert_eq!(leds[2], maslight_core::Rgb8::new(255, 255, 255));
    for (i, c) in leds.iter().enumerate() {
        if i != 2 {
            assert_eq!(*c, maslight_core::Rgb8::BLACK, "LED {i} should be dark");
        }
    }
}

#[test]
fn a_held_colour_overrides_capture() {
    let _serial = serial();
    let engine = EngineHandle::spawn(config_with(test_profile(vec![DeviceConfig::Null])));
    engine.hold_color(Some(maslight_core::Rgb8::new(10, 20, 30)));

    let leds = wait_for(Duration::from_secs(3), || {
        let s = engine.status();
        (s.leds.len() == 4 && s.leds[0] == maslight_core::Rgb8::new(10, 20, 30)).then_some(s.leds)
    })
    .expect("held colour never appeared");

    assert!(leds
        .iter()
        .all(|c| *c == maslight_core::Rgb8::new(10, 20, 30)));
}

#[test]
fn applying_a_new_profile_resizes_the_chain_without_restarting() {
    let _serial = serial();
    let engine = EngineHandle::spawn(config_with(test_profile(vec![DeviceConfig::Null])));
    wait_for(Duration::from_secs(5), || {
        (engine.status().led_count == 4).then_some(())
    })
    .expect("engine never started");

    let mut bigger = test_profile(vec![DeviceConfig::Null]);
    bigger.layout = Layout::from_wizard(&WizardParams {
        display: String::from("test"),
        counts: EdgeCounts {
            top: 10,
            right: 6,
            bottom: 10,
            left: 6,
        },
        ..Default::default()
    });
    engine.apply(config_with(bigger));

    let count = wait_for(Duration::from_secs(5), || {
        let s = engine.status();
        (s.led_count == 32 && s.leds.len() == 32).then_some(s.led_count)
    })
    .expect("engine did not pick up the new layout");
    assert_eq!(count, 32);
    assert!(engine.status().running);
}

#[test]
fn an_unreachable_device_does_not_stop_the_engine() {
    let _serial = serial();
    // Port 1 on localhost has nothing listening. UDP will not error on send,
    // but the profile also carries a serial device that cannot open, which is
    // the failure path we care about.
    let engine = EngineHandle::spawn(config_with(test_profile(vec![
        DeviceConfig::Serial {
            port: String::from("COM_DOES_NOT_EXIST"),
            baud: 115_200,
            protocol: maslight_core::SerialProtocol::Adalight,
        },
        DeviceConfig::Null,
    ])));

    let status = wait_for(Duration::from_secs(5), || {
        let s = engine.status();
        (s.leds.iter().any(|c| c.b > 0)).then_some(s)
    })
    .expect("engine stopped because one device was unavailable");

    assert!(status.running);
    assert!(
        status.devices.iter().any(|d| !d.connected),
        "the broken device should be reported as disconnected"
    );
    assert!(
        status.devices.iter().any(|d| d.connected),
        "the working device should still be connected"
    );
}

/// Capture that will not start must be tried again, not given up on.
///
/// This is the failure this test was written for: if `start` failed once, the
/// engine kept no sources, carried on with nothing to read, and the lights
/// stayed dark until somebody changed a setting or restarted the application.
/// The causes are all temporary. Desktop Duplication is exclusive per output,
/// so a previous instance still shutting down, a game in exclusive fullscreen
/// or a driver reset all make start fail for a few seconds and then stop
/// mattering.
#[test]
fn capture_that_refuses_to_start_is_tried_again() {
    let _serial = serial();
    // Two failures, then the backend behaves. The engine should get there on
    // its own without anyone touching the configuration.
    maslight_capture::test_source::fail_next_starts(2);

    let engine = EngineHandle::spawn(config_with(test_profile(vec![DeviceConfig::Null])));

    // Long enough for two retries at the engine's interval, and no longer.
    let status = wait_for(Duration::from_secs(12), || {
        let s = engine.status();
        (s.capture_backend != "none" && s.leds.iter().any(|c| c.r > 0 || c.g > 0 || c.b > 0))
            .then_some(s)
    })
    .expect("the engine never retried a capture that failed to start");

    assert_eq!(
        maslight_capture::test_source::remaining_failing_starts(),
        0,
        "both failures should have been spent on real attempts"
    );
    assert!(status.running);
    assert_eq!(
        status.capture_backend, "Test",
        "the backend should be the one that eventually started"
    );
}
