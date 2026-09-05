//! What a frame actually costs.
//!
//! The readme makes performance claims. This is where they come from, so they
//! can be re-measured on any machine rather than taken on trust:
//!
//! ```text
//! cargo run --release -p maslight-core --example bench
//! ```
//!
//! Release matters: a debug build is ten times slower and says nothing useful
//! about what ships.

use std::time::Instant;

use maslight_core::layout::{EdgeCounts, WizardParams};
use maslight_core::{
    ColorPipeline, ColorSettings, FrameView, Insets, Layout, PixelFormat, Reducer, Rgb,
};

/// A synthetic desktop: a gradient, so the reducer cannot be helped by a
/// uniform image and the cache behaves like it would on a real screen.
fn desktop(width: u32, height: u32) -> Vec<u8> {
    let mut data = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let p = ((y * width + x) * 4) as usize;
            data[p] = (x * 255 / width.max(1)) as u8;
            data[p + 1] = (y * 255 / height.max(1)) as u8;
            data[p + 2] = ((x + y) * 255 / (width + height).max(1)) as u8;
            data[p + 3] = 255;
        }
    }
    data
}

fn layout_for(leds: usize) -> Layout {
    let per_edge = (leds / 4).max(1) as u32;
    Layout::from_wizard(&WizardParams {
        display: String::from("bench"),
        counts: EdgeCounts {
            top: per_edge,
            right: per_edge,
            bottom: per_edge,
            left: per_edge,
        },
        ..Default::default()
    })
}

fn time(label: &str, iterations: u32, mut body: impl FnMut()) -> f64 {
    // One warm pass so the first measurement is not paying for page faults.
    body();
    let started = Instant::now();
    for _ in 0..iterations {
        body();
    }
    let per_call = started.elapsed().as_secs_f64() * 1000.0 / iterations as f64;
    println!("  {label:<44} {per_call:>8.3} ms");
    per_call
}

fn main() {
    if cfg!(debug_assertions) {
        println!("warning: this is a debug build, the numbers mean nothing");
        println!("         run with --release\n");
    }

    println!("reduction, the per frame cost of sampling the screen");
    let mut worst_reduce = 0.0f64;
    for (width, height) in [(1920u32, 1080u32), (2560, 1440), (3840, 2160)] {
        let data = desktop(width, height);
        let view = FrameView::new(&data, width, height, width as usize * 4, PixelFormat::Bgra8);
        for leds in [60usize, 120, 300] {
            let layout = layout_for(leds);
            let rects: Vec<_> = layout.leds.iter().map(|l| l.rect).collect();
            let reducer = Reducer::new(16);
            let mut out = Vec::new();
            let ms = time(
                &format!("{width}x{height}, {} LEDs", rects.len()),
                200,
                || reducer.reduce(&view, &rects, Insets::default(), &mut out),
            );
            worst_reduce = worst_reduce.max(ms);
        }
    }

    println!("\ncolour pipeline, the per frame cost of everything after that");
    let mut worst_pipeline = 0.0f64;
    for leds in [60usize, 300, 1200] {
        let input = vec![Rgb::new(0.4, 0.6, 0.2); leds];
        let mut pipeline = ColorPipeline::new(ColorSettings {
            // Every stage on, so this is the worst case rather than the best.
            gamma: 2.2,
            vibrance: 0.3,
            temperature_k: 3200.0,
            dithering: true,
            power_limit_a: Some(2.0),
            ..Default::default()
        });
        let ms = time(&format!("{leds} LEDs, every stage on"), 2000, || {
            pipeline.process(&input, 1.0 / 60.0);
        });
        worst_pipeline = worst_pipeline.max(ms);
    }

    println!("\nletterbox detection, which only runs when it is enabled");
    let data = desktop(1920, 1080);
    let view = FrameView::new(&data, 1920, 1080, 1920 * 4, PixelFormat::Bgra8);
    time("1920x1080", 200, || {
        maslight_core::detect_black_bars(&view, 0.006);
    });

    // The number worth quoting is a real configuration, measured end to end,
    // rather than the worst row of one table added to the worst row of another.
    let data = desktop(1920, 1080);
    let view = FrameView::new(&data, 1920, 1080, 1920 * 4, PixelFormat::Bgra8);
    let layout = layout_for(60);
    let rects: Vec<_> = layout.leds.iter().map(|l| l.rect).collect();
    let reducer = Reducer::new(16);
    let mut pipeline = ColorPipeline::new(ColorSettings {
        gamma: 2.2,
        vibrance: 0.3,
        temperature_k: 3200.0,
        dithering: true,
        power_limit_a: Some(2.0),
        ..Default::default()
    });
    let mut linear = Vec::new();
    reducer.reduce(&view, &rects, Insets::default(), &mut linear);
    let iterations = 2000;
    let started = Instant::now();
    for _ in 0..iterations {
        reducer.reduce(&view, &rects, Insets::default(), &mut linear);
        pipeline.process(&linear, 1.0 / 60.0);
    }
    let typical = started.elapsed().as_secs_f64() * 1000.0 / iterations as f64;

    let budget = 1000.0 / 60.0;
    let worst = worst_reduce + worst_pipeline;
    println!("\nper frame, reduce and colour pipeline together");
    println!(
        "  1920x1080, 60 LEDs      {typical:.3} ms, {:.2}% of one core at 60 fps",
        typical / budget * 100.0
    );
    println!(
        "  3840x2160, 1200 LEDs    {worst:.3} ms, {:.2}% of one core at 60 fps",
        worst / budget * 100.0
    );
    println!("the capture and the readback are on top, and are the larger half");
}
