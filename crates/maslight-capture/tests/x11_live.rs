//! A live X11 capture test.
//!
//! This is the only way the Linux path gets exercised for real: it paints a
//! known colour on the root window, captures it, and checks the reducer sees
//! that colour. Continuous integration runs it under Xvfb.
//!
//! It is gated behind `MASLIGHT_X11_TEST=1` because it paints the root window,
//! and a test suite has no business redecorating somebody's desktop. Without
//! the variable it reports that it was skipped and passes.

#![cfg(target_os = "linux")]

use std::time::Duration;

use maslight_capture::{create_backend, FrameStatus};
use maslight_core::layout::Rect;
use maslight_core::{CaptureBackendKind, CaptureSettings, Insets, Reducer, Rgb};
use x11rb::rust_connection::RustConnection;

fn enabled() -> bool {
    std::env::var("MASLIGHT_X11_TEST").as_deref() == Ok("1") && std::env::var("DISPLAY").is_ok()
}

/// Paint the whole root window one colour.
///
/// The connection comes back with the size, and the caller has to keep it
/// alive. This is not tidiness, it is the whole reason an earlier version of
/// this test failed on CI: an X server resets itself when its **last** client
/// disconnects, and part of a reset is repainting the root window with its
/// default background. Under Xvfb there is no window manager and no desktop,
/// so the painting client is usually the only client. Dropping the connection
/// here therefore erased the red before the capture backend could connect, and
/// the reducer correctly reported black.
fn paint_root(colour: u32) -> Result<(RustConnection, u16, u16), String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt, CreateGCAux, Rectangle};

    let (conn, screen_num) = x11rb::connect(None).map_err(|e| e.to_string())?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    let width = screen.width_in_pixels;
    let height = screen.height_in_pixels;

    let gc = conn.generate_id().map_err(|e| e.to_string())?;
    conn.create_gc(gc, root, &CreateGCAux::new().foreground(colour))
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;

    conn.poly_fill_rectangle(
        root,
        gc,
        &[Rectangle {
            x: 0,
            y: 0,
            width,
            height,
        }],
    )
    .map_err(|e| e.to_string())?
    .check()
    .map_err(|e| e.to_string())?;

    conn.free_gc(gc).map_err(|e| e.to_string())?;
    conn.flush().map_err(|e| e.to_string())?;
    Ok((conn, width, height))
}

#[test]
fn x11_capture_reads_what_is_on_the_root_window() {
    if !enabled() {
        eprintln!("skipped: set MASLIGHT_X11_TEST=1 with a DISPLAY to run this");
        return;
    }

    // Pure red, so a channel swap in the backend cannot pass unnoticed. The
    // connection is bound rather than dropped so the server does not reset and
    // wipe the paint; see paint_root.
    let (_painter, width, height) =
        paint_root(0x00ff_0000).expect("could not paint the root window");

    let mut backend =
        create_backend(CaptureBackendKind::X11).expect("no X11 backend on this display");
    let displays = backend.displays().expect("no displays");
    assert!(
        !displays.is_empty(),
        "X11 should report at least one output"
    );

    backend
        .start(None, &CaptureSettings::default())
        .expect("capture did not start");

    let size = backend.current_size().expect("no size after start");
    assert_eq!(
        (size.0 as u16, size.1 as u16),
        (width, height),
        "the captured surface should be the whole screen"
    );

    let reducer = Reducer::new(16);
    let mut out: Vec<Rgb> = Vec::new();
    let mut got = false;

    // A couple of attempts, because the first grab can land before the fill
    // has been processed by the server.
    for _ in 0..5 {
        let status = backend
            .next_frame(Duration::from_millis(200), &mut |view| {
                reducer.reduce(view, &[Rect::FULL], Insets::default(), &mut out);
                got = true;
            })
            .expect("capture failed");
        if matches!(status, FrameStatus::New) && got && out.first().is_some_and(|c| c.r > 0.5) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    assert!(got, "no frame was delivered");
    let colour = out[0];
    assert!(
        colour.r > 0.9,
        "the screen was painted red, the reducer saw {colour:?}"
    );
    assert!(
        colour.g < 0.05 && colour.b < 0.05,
        "red must not arrive as another channel: {colour:?}"
    );

    backend.stop();
}

#[test]
fn x11_capture_survives_a_stop_and_a_restart() {
    if !enabled() {
        eprintln!("skipped: set MASLIGHT_X11_TEST=1 with a DISPLAY to run this");
        return;
    }
    let mut backend = create_backend(CaptureBackendKind::X11).expect("no X11 backend");
    let settings = CaptureSettings::default();

    for _ in 0..3 {
        backend.start(None, &settings).expect("start failed");
        let mut seen = false;
        backend
            .next_frame(Duration::from_millis(200), &mut |view| {
                seen = view.is_valid();
            })
            .expect("capture failed");
        assert!(seen, "a restarted session should still deliver frames");
        backend.stop();
    }
}
