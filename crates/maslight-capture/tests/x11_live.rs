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

use std::sync::OnceLock;
use std::time::Duration;

use maslight_capture::{create_backend, FrameStatus};
use maslight_core::layout::Rect;
use maslight_core::{CaptureBackendKind, CaptureSettings, Insets, Reducer, Rgb};
use x11rb::connection::Connection;
use x11rb::rust_connection::RustConnection;

fn enabled() -> bool {
    std::env::var("MASLIGHT_X11_TEST").as_deref() == Ok("1") && std::env::var("DISPLAY").is_ok()
}

/// One connection, opened once, never closed.
///
/// An X server resets itself when its **last** client disconnects: it drops
/// every connection and repaints the root window with its default background.
/// Under Xvfb there is no window manager and no desktop, so whichever client
/// this test binary happens to have open is usually the only one, and every
/// gap between connections is a reset.
///
/// That cost two separate CI failures. First the painting connection was
/// dropped on return, so the server wiped the red before the capture backend
/// could connect and the reducer correctly reported black. Fixing that by
/// holding the painter alive only moved the problem: the painter now died at
/// the end of the first test, and the second test connected into the middle of
/// the reset and got `Connection reset by peer`.
///
/// A static is never dropped, so this client outlives every test in the binary
/// and the server has no reason to reset while any of them are running.
fn keepalive() -> &'static (RustConnection, usize) {
    static CONN: OnceLock<(RustConnection, usize)> = OnceLock::new();
    CONN.get_or_init(|| x11rb::connect(None).expect("could not open the keepalive connection"))
}

/// Paint the whole root window one colour.
fn paint_root(colour: u32) -> Result<(u16, u16), String> {
    use x11rb::protocol::xproto::{ConnectionExt, CreateGCAux, Rectangle};

    let (conn, screen_num) = keepalive();
    let screen = &conn.setup().roots[*screen_num];
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
    Ok((width, height))
}

#[test]
fn x11_capture_reads_what_is_on_the_root_window() {
    if !enabled() {
        eprintln!("skipped: set MASLIGHT_X11_TEST=1 with a DISPLAY to run this");
        return;
    }

    // Pure red, so a channel swap in the backend cannot pass unnoticed.
    let (width, height) = paint_root(0x00ff_0000).expect("could not paint the root window");

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
    // Hold the server open for the duration; see keepalive.
    keepalive();

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
