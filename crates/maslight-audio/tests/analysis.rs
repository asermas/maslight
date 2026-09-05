use maslight_audio::analysis::{Analyser, WINDOW};
use maslight_audio::{effects, AudioEffect, AudioSettings, Palette, Spectrum};

const RATE: f32 = 48_000.0;

fn settings() -> AudioSettings {
    AudioSettings {
        // No smoothing, so one call is one answer.
        attack_ms: 0.0,
        release_ms: 0.0,
        ..Default::default()
    }
}

/// A full scale sine at `hz`.
fn sine(hz: f32) -> Vec<f32> {
    (0..WINDOW)
        .map(|i| (std::f32::consts::TAU * hz * i as f32 / RATE).sin())
        .collect()
}

#[test]
fn a_loud_tone_reads_as_loud() {
    // Energy is broadband loudness, not the mean of the bands: a full scale
    // sine fills one band in sixteen but it is not a quiet sound.
    let mut a = Analyser::new(RATE, 16);
    let s = a.process(&sine(1000.0), &settings(), 1.0 / 60.0);
    assert!(s.energy > 0.7, "a full scale tone read as {}", s.energy);
}

#[test]
fn silence_produces_no_light() {
    let mut a = Analyser::new(RATE, 16);
    let s = a.process(&vec![0.0; WINDOW], &settings(), 1.0 / 60.0);
    assert_eq!(s.bands.len(), 16);
    assert!(
        s.bands.iter().all(|b| *b < 0.001),
        "silence should be dark: {:?}",
        s.bands
    );
    assert!(s.energy < 0.001);
    assert!(!s.beat);
}

#[test]
fn a_tone_lands_in_the_band_that_contains_it() {
    let mut a = Analyser::new(RATE, 16);
    let s = a.process(&sine(1000.0), &settings(), 1.0 / 60.0);

    let loudest = s
        .bands
        .iter()
        .enumerate()
        .max_by(|x, y| x.1.partial_cmp(y.1).unwrap())
        .map(|(i, _)| i)
        .unwrap();

    // Bands run from 40 Hz to 16 kHz on a log scale. 1 kHz is a bit over half
    // way, so it must not be in the first or last quarter.
    assert!(
        (5..=11).contains(&loudest),
        "1 kHz landed in band {loudest} of 16: {:?}",
        s.bands
    );
    assert!(s.bands[loudest] > 0.5, "a full scale tone should be loud");
}

#[test]
fn a_low_tone_and_a_high_tone_land_in_different_places() {
    let mut a = Analyser::new(RATE, 16);
    let low = a.process(&sine(80.0), &settings(), 1.0 / 60.0);
    let low_peak = peak(&low);

    let mut a = Analyser::new(RATE, 16);
    let high = a.process(&sine(8000.0), &settings(), 1.0 / 60.0);
    let high_peak = peak(&high);

    assert!(
        low_peak < high_peak,
        "80 Hz landed at {low_peak}, 8 kHz at {high_peak}"
    );
    assert!(low_peak <= 3, "80 Hz should be near the bottom: {low_peak}");
    assert!(high_peak >= 12, "8 kHz should be near the top: {high_peak}");
}

fn peak(s: &Spectrum) -> usize {
    s.bands
        .iter()
        .enumerate()
        .max_by(|x, y| x.1.partial_cmp(y.1).unwrap())
        .map(|(i, _)| i)
        .unwrap()
}

#[test]
fn quiet_audio_stays_below_loud_audio() {
    let mut a = Analyser::new(RATE, 16);
    let loud = a.process(&sine(1000.0), &settings(), 1.0 / 60.0).energy;

    let quiet_samples: Vec<f32> = sine(1000.0).iter().map(|s| s * 0.01).collect();
    let mut a = Analyser::new(RATE, 16);
    let quiet = a.process(&quiet_samples, &settings(), 1.0 / 60.0).energy;

    assert!(quiet < loud, "quiet {quiet} should be under loud {loud}");
    assert!(quiet >= 0.0);
}

#[test]
fn release_is_slower_than_attack() {
    let mut s = settings();
    s.attack_ms = 10.0;
    s.release_ms = 400.0;
    let mut a = Analyser::new(RATE, 16);

    // Rise towards a tone.
    let tone = sine(1000.0);
    let mut rising = 0.0;
    for _ in 0..6 {
        rising = a.process(&tone, &s, 1.0 / 60.0).energy;
    }
    assert!(rising > 0.5, "should have risen by now: {rising}");

    // Then fall back into silence for the same number of frames.
    let quiet = vec![0.0; WINDOW];
    let mut falling = rising;
    for _ in 0..6 {
        falling = a.process(&quiet, &s, 1.0 / 60.0).energy;
    }
    assert!(falling > 0.0, "release should not be instant: {falling}");
    assert!(falling < rising, "it still has to fall: {falling}");
}

#[test]
fn smoothing_is_frame_rate_independent() {
    let mut s = settings();
    s.attack_ms = 100.0;
    s.release_ms = 100.0;
    let tone = sine(1000.0);

    let mut fast = Analyser::new(RATE, 16);
    let mut fast_level = 0.0;
    for _ in 0..8 {
        fast_level = fast.process(&tone, &s, 1.0 / 120.0).energy;
    }

    let mut slow = Analyser::new(RATE, 16);
    let mut slow_level = 0.0;
    for _ in 0..4 {
        slow_level = slow.process(&tone, &s, 1.0 / 60.0).energy;
    }

    assert!(
        (fast_level - slow_level).abs() < 0.02,
        "120 fps reached {fast_level}, 60 fps reached {slow_level}"
    );
}

#[test]
fn a_pulse_train_is_heard_as_beats() {
    let mut s = settings();
    s.attack_ms = 5.0;
    s.release_ms = 90.0;
    s.beat_sensitivity = 1.0;
    let mut a = Analyser::new(RATE, 16);

    let kick = sine(60.0);
    let quiet = vec![0.0; WINDOW];

    let mut beats = 0;
    // Roughly 120 bpm at 60 fps: two loud frames, then ten quiet ones.
    for _ in 0..12 {
        for _ in 0..2 {
            if a.process(&kick, &s, 1.0 / 60.0).beat {
                beats += 1;
            }
        }
        for _ in 0..10 {
            if a.process(&quiet, &s, 1.0 / 60.0).beat {
                beats += 1;
            }
        }
    }

    assert!(
        (6..=24).contains(&beats),
        "expected roughly one beat per pulse, got {beats}"
    );
}

#[test]
fn every_band_can_be_reached() {
    // A sweep should light every band at some point. A band that never moves
    // means the edges collapsed and some LEDs would sit dark forever.
    let mut a = Analyser::new(RATE, 16);
    let mut seen = [false; 16];
    let mut hz = 45.0;
    while hz < 15_000.0 {
        let s = a.process(&sine(hz), &settings(), 1.0 / 60.0);
        for (i, level) in s.bands.iter().enumerate() {
            if *level > 0.3 {
                seen[i] = true;
            }
        }
        hz *= 1.12;
    }
    assert!(
        seen.iter().all(|v| *v),
        "some bands never lit: {:?}",
        seen.iter()
            .enumerate()
            .filter(|(_, v)| !**v)
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_short_buffer_is_refused_rather_than_read_out_of_bounds() {
    let mut a = Analyser::new(RATE, 16);
    let s = a.process(&[0.0; 10], &settings(), 1.0 / 60.0);
    assert_eq!(s.bands.len(), 16);
    assert_eq!(s.energy, 0.0);
}

// --- effects -------------------------------------------------------------

#[test]
fn every_effect_fills_the_whole_chain() {
    let spectrum = Spectrum {
        bands: vec![0.5; 16],
        energy: 0.5,
        beat: false,
        beat_envelope: 0.0,
    };
    for effect in [
        AudioEffect::Spectrum,
        AudioEffect::Energy,
        AudioEffect::Wave,
        AudioEffect::Scroll,
    ] {
        let mut out = Vec::new();
        effects::render(
            effect,
            &spectrum,
            &Palette::default(),
            0.3,
            0.0,
            64,
            &mut out,
        );
        assert_eq!(out.len(), 64, "{effect:?} produced the wrong length");
        assert!(
            out.iter().any(|c| c.max_channel() > 0.05),
            "{effect:?} produced nothing visible"
        );
    }
}

#[test]
fn silence_leaves_the_strip_dark_in_every_effect() {
    let spectrum = Spectrum {
        bands: vec![0.0; 16],
        energy: 0.0,
        beat: false,
        beat_envelope: 0.0,
    };
    for effect in [
        AudioEffect::Spectrum,
        AudioEffect::Energy,
        AudioEffect::Wave,
        AudioEffect::Scroll,
    ] {
        let mut out = Vec::new();
        effects::render(
            effect,
            &spectrum,
            &Palette::default(),
            0.3,
            0.0,
            32,
            &mut out,
        );
        assert!(
            out.iter().all(|c| c.max_channel() < 0.01),
            "{effect:?} lit up on silence"
        );
    }
}

#[test]
fn the_spectrum_effect_is_mirrored_around_the_middle() {
    // Bass loud, treble quiet: both ends of the strip should be bright and the
    // middle dark.
    let mut bands = vec![0.0; 16];
    bands[0] = 1.0;
    bands[1] = 0.9;
    let spectrum = Spectrum {
        bands,
        energy: 0.2,
        beat: false,
        beat_envelope: 0.0,
    };
    let mut out = Vec::new();
    effects::render(
        AudioEffect::Spectrum,
        &spectrum,
        &Palette::default(),
        0.0,
        0.0,
        64,
        &mut out,
    );
    let first = out[0].max_channel();
    let last = out[63].max_channel();
    let middle = out[32].max_channel();
    assert!(first > 0.5 && last > 0.5, "ends: {first} and {last}");
    assert!(middle < 0.2, "middle should be dark: {middle}");
}

#[test]
fn a_beat_brightens_the_frame() {
    let base = Spectrum {
        bands: vec![0.3; 16],
        energy: 0.3,
        beat: false,
        beat_envelope: 0.0,
    };
    let hit = Spectrum {
        beat: true,
        beat_envelope: 1.0,
        ..base.clone()
    };

    let mut quiet = Vec::new();
    let mut loud = Vec::new();
    effects::render(
        AudioEffect::Energy,
        &base,
        &Palette::default(),
        0.5,
        0.0,
        8,
        &mut quiet,
    );
    effects::render(
        AudioEffect::Energy,
        &hit,
        &Palette::default(),
        0.5,
        0.0,
        8,
        &mut loud,
    );

    assert!(
        loud[0].max_channel() > quiet[0].max_channel(),
        "a beat should be visible"
    );
}

#[test]
fn hsv_gives_the_primaries_where_they_belong() {
    let red = effects::hsv(0.0, 1.0, 1.0);
    assert!(red.r > 0.99 && red.g < 0.01 && red.b < 0.01, "{red:?}");
    let green = effects::hsv(1.0 / 3.0, 1.0, 1.0);
    assert!(green.g > 0.99 && green.r < 0.01, "{green:?}");
    let blue = effects::hsv(2.0 / 3.0, 1.0, 1.0);
    assert!(blue.b > 0.99 && blue.g < 0.01, "{blue:?}");
    let white = effects::hsv(0.5, 0.0, 1.0);
    assert!(
        white.r > 0.99 && white.g > 0.99 && white.b > 0.99,
        "{white:?}"
    );
}

#[test]
fn corrupt_settings_are_clamped() {
    let mut s = AudioSettings {
        bands: 5000,
        gain: f32::NAN,
        floor_db: 10.0,
        ceiling_db: -200.0,
        ..Default::default()
    };
    s.sanitise();
    assert_eq!(s.bands, 64);
    assert!(s.gain.is_finite());
    assert!(s.ceiling_db > s.floor_db);
}
