//! Watch what the audio engine hears.
//!
//! Run it, play something, and the bars should move. If they stay flat the
//! problem is capture, not the effects.
//!
//! ```text
//! cargo run -p maslight-audio --example listen
//! cargo run -p maslight-audio --example listen -- 10   # seconds
//! ```

use std::time::{Duration, Instant};

use maslight_audio::{AudioEngine, AudioSettings};
use maslight_core::Rgb;

fn main() {
    let seconds: f32 = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(6.0);

    println!("devices:");
    for name in maslight_audio::list_devices() {
        println!("  {name}");
    }

    let settings = AudioSettings::default();
    let mut engine = match AudioEngine::start(&settings) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("cannot start audio capture: {e}");
            std::process::exit(1);
        }
    };
    println!("listening on {}", engine.device_name());

    let mut leds: Vec<Rgb> = Vec::new();
    let mut last = Instant::now();
    let started = Instant::now();
    let mut peak_energy = 0.0f32;
    let mut beats = 0u32;

    while started.elapsed().as_secs_f32() < seconds {
        std::thread::sleep(Duration::from_millis(50));
        let now = Instant::now();
        let dt = (now - last).as_secs_f32();
        last = now;

        if !engine.frame(&settings, 32, dt, &mut leds) {
            continue;
        }
        let spectrum = engine.spectrum().clone();
        peak_energy = peak_energy.max(spectrum.energy);
        if spectrum.beat {
            beats += 1;
        }

        let bars: String = spectrum
            .bands
            .iter()
            .map(|level| {
                let blocks = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
                blocks[((level * 9.0).round() as usize).min(9)]
            })
            .collect();
        println!(
            "[{bars}] energy {:.2} {}",
            spectrum.energy,
            if spectrum.beat { "beat" } else { "" }
        );
    }

    println!("peak energy {peak_energy:.2}, {beats} beats in {seconds:.0}s");
    if peak_energy < 0.01 {
        println!("nothing was heard: check that audio is actually playing");
    }
}
