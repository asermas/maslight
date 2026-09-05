//! Capture probe.
//!
//! Lists the displays, captures a short burst from one of them and prints what
//! the reducer sees, along with timing. Run it when a capture backend misbehaves
//! on a new machine:
//!
//! ```text
//! cargo run -p maslight-capture --example probe
//! ```

use std::time::{Duration, Instant};

use maslight_capture::{create_backend, CaptureBackend, FrameStatus};
use maslight_core::layout::Rect;
use maslight_core::{CaptureBackendKind, CaptureSettings, Insets, Reducer, Rgb};

fn main() {
    let kind = match std::env::args().nth(1).as_deref() {
        Some("test") => CaptureBackendKind::Test,
        Some("dxgi") => CaptureBackendKind::Dxgi,
        Some("x11") => CaptureBackendKind::X11,
        _ => CaptureBackendKind::Auto,
    };

    let mut backend: Box<dyn CaptureBackend> = match create_backend(kind) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("no backend: {e}");
            std::process::exit(1);
        }
    };
    println!("backend: {:?}", backend.kind());

    let displays = backend.displays().expect("display enumeration failed");
    for d in &displays {
        println!(
            "  display {} [{}] {}x{} at {},{}{}",
            d.id,
            d.label,
            d.width,
            d.height,
            d.x,
            d.y,
            if d.primary { " (primary)" } else { "" }
        );
    }

    let settings = CaptureSettings::default();
    backend
        .start(None, &settings)
        .expect("capture start failed");
    println!("capturing {:?}", backend.current_size());

    // Four probes: the middle of each screen edge.
    let rects = [
        Rect::new(0.0, 0.4, 0.15, 0.2),
        Rect::new(0.4, 0.0, 0.2, 0.15),
        Rect::new(0.85, 0.4, 0.15, 0.2),
        Rect::new(0.4, 0.85, 0.2, 0.15),
    ];
    let reducer = Reducer::new(settings.sample_grid);
    let mut out: Vec<Rgb> = Vec::new();

    let mut frames = 0u32;
    let mut unchanged = 0u32;
    let mut timeouts = 0u32;
    let mut worst = Duration::ZERO;
    let started = Instant::now();

    while frames < 60 && started.elapsed() < Duration::from_secs(5) {
        let t0 = Instant::now();
        let status = backend
            .next_frame(Duration::from_millis(200), &mut |view| {
                reducer.reduce(view, &rects, Insets::default(), &mut out);
            })
            .expect("capture failed");
        let dt = t0.elapsed();
        match status {
            FrameStatus::New => {
                frames += 1;
                worst = worst.max(dt);
            }
            FrameStatus::Unchanged => unchanged += 1,
            FrameStatus::Timeout => timeouts += 1,
            FrameStatus::Lost => {
                println!("capture lost, restarting");
                backend.start(None, &settings).ok();
            }
        }
    }

    let elapsed = started.elapsed();
    println!(
        "{frames} frames, {unchanged} unchanged, {timeouts} timeouts in {:.2}s",
        elapsed.as_secs_f32()
    );
    println!(
        "worst frame latency: {:.2} ms",
        worst.as_secs_f32() * 1000.0
    );
    println!("edge colours in linear light (left, top, right, bottom):");
    for (i, c) in out.iter().enumerate() {
        println!(
            "  {i}: r={:.3} g={:.3} b={:.3}  -> 8-bit {:3} {:3} {:3}",
            c.r,
            c.g,
            c.b,
            (c.r.clamp(0.0, 1.0) * 255.0) as u8,
            (c.g.clamp(0.0, 1.0) * 255.0) as u8,
            (c.b.clamp(0.0, 1.0) * 255.0) as u8
        );
    }
}
