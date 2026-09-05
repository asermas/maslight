use maslight_core::color::{
    apply_luminance_policy, kelvin_multipliers, smoothing_alpha, split_white, tone_map,
};
use maslight_core::{ColorPipeline, ColorSettings, LuminancePolicy, Rgb, RgbwMode};

/// Settings with every time-dependent stage disabled, so a single call is
/// deterministic.
fn plain() -> ColorSettings {
    ColorSettings {
        smoothing_ms: 0.0,
        dithering: false,
        ..Default::default()
    }
}

#[test]
fn white_and_black_survive_the_default_pipeline() {
    let mut p = ColorPipeline::new(plain());
    let frame = p.process(&[Rgb::WHITE, Rgb::BLACK], 1.0 / 60.0);
    assert_eq!(frame.rgb[0], maslight_core::Rgb8::new(255, 255, 255));
    assert_eq!(frame.rgb[1], maslight_core::Rgb8::new(0, 0, 0));
}

#[test]
fn gamma_of_one_is_a_linear_pass_through() {
    let mut p = ColorPipeline::new(plain());
    let frame = p.process(&[Rgb::splat(0.5)], 1.0 / 60.0);
    // 0.5 linear -> 127 or 128 out of 255, not the 188 an sRGB encode gives.
    assert!(
        (frame.rgb[0].r as i32 - 128).abs() <= 1,
        "expected ~128, got {}",
        frame.rgb[0].r
    );
}

#[test]
fn gamma_above_one_lifts_midtones() {
    let mut settings = plain();
    settings.gamma = 2.2;
    let mut p = ColorPipeline::new(settings);
    let frame = p.process(&[Rgb::splat(0.5)], 1.0 / 60.0);
    assert!(frame.rgb[0].r > 180, "got {}", frame.rgb[0].r);
}

#[test]
fn six_thousand_five_hundred_kelvin_is_neutral() {
    let m = kelvin_multipliers(6500.0);
    for c in m {
        assert!((c - 1.0).abs() < 1e-5, "6500K should be neutral, got {m:?}");
    }
}

#[test]
fn warm_white_point_pulls_blue_down_and_red_up() {
    let warm = kelvin_multipliers(2700.0);
    assert!(warm[2] < 0.5, "blue should drop hard at 2700K: {warm:?}");
    assert!(warm[0] >= 1.0, "red should not drop at 2700K: {warm:?}");

    let cool = kelvin_multipliers(9000.0);
    assert!(cool[2] > 1.0, "blue should rise at 9000K: {cool:?}");
    assert!(cool[0] < 1.0, "red should drop at 9000K: {cool:?}");
}

#[test]
fn saturation_zero_produces_grey() {
    let mut settings = plain();
    settings.saturation = 0.0;
    let mut p = ColorPipeline::new(settings);
    let frame = p.process(&[Rgb::new(1.0, 0.0, 0.0)], 1.0 / 60.0);
    let c = frame.rgb[0];
    assert_eq!(c.r, c.g);
    assert_eq!(c.g, c.b);
}

#[test]
fn dead_zone_kills_dark_scenes_and_minimum_level_lifts_them() {
    let dark = Rgb::splat(0.01);
    let dead = apply_luminance_policy(dark, 0.05, LuminancePolicy::DeadZone);
    assert_eq!(dead, Rgb::BLACK);

    let lifted = apply_luminance_policy(dark, 0.05, LuminancePolicy::MinimumLevel);
    assert!((lifted.luminance() - 0.05).abs() < 1e-4);

    // Pure black has no hue to preserve, so it becomes a neutral floor.
    let floor = apply_luminance_policy(Rgb::BLACK, 0.05, LuminancePolicy::MinimumLevel);
    assert_eq!(floor, Rgb::splat(0.05));
}

#[test]
fn smoothing_is_frame_rate_independent() {
    // Reaching the same point in time must give the same result whether we
    // took two big steps or four small ones.
    let step = |alpha: f32, from: f32| from + (1.0 - from) * alpha;

    let a60 = smoothing_alpha(100.0, 1.0 / 60.0);
    let mut v60 = 0.0;
    for _ in 0..4 {
        v60 = step(a60, v60);
    }

    let a30 = smoothing_alpha(100.0, 1.0 / 30.0);
    let mut v30 = 0.0;
    for _ in 0..2 {
        v30 = step(a30, v30);
    }

    assert!(
        (v60 - v30).abs() < 0.01,
        "60 fps reached {v60}, 30 fps reached {v30}"
    );
}

#[test]
fn smoothing_converges_towards_the_target() {
    let mut settings = plain();
    settings.smoothing_ms = 100.0;
    let mut p = ColorPipeline::new(settings);

    // First frame is shown immediately, so start from black and step to white.
    p.process(&[Rgb::BLACK], 1.0 / 60.0);
    let mut last = 0u8;
    for _ in 0..30 {
        let f = p.process(&[Rgb::WHITE], 1.0 / 60.0);
        assert!(f.rgb[0].r >= last, "smoothing must be monotonic here");
        last = f.rgb[0].r;
    }
    assert!(last > 250, "should be nearly white after 0.5 s, got {last}");
}

#[test]
fn power_limit_scales_the_whole_strip() {
    let mut settings = plain();
    // 60 LEDs at full white is 60 * 3 * 20 mA = 3.6 A. Ask for 1 A.
    settings.power_limit_a = Some(1.0);
    let mut p = ColorPipeline::new(settings);
    let frame = p.process(&vec![Rgb::WHITE; 60], 1.0 / 60.0);
    let amps = frame.estimated_current_a(20.0);
    assert!(amps <= 1.05, "limiter let {amps} A through");
    assert!(amps > 0.9, "limiter was too aggressive: {amps} A");
}

#[test]
fn dithering_averages_out_to_the_true_value_over_time() {
    let mut settings = plain();
    settings.dithering = true;
    let mut p = ColorPipeline::new(settings);

    // A level that sits exactly between two 8-bit codes.
    let target = 10.5f32 / 255.0;
    let mut sum = 0u32;
    let frames = 100;
    for _ in 0..frames {
        let f = p.process(&[Rgb::splat(target)], 1.0 / 60.0);
        sum += f.rgb[0].r as u32;
    }
    let mean = sum as f32 / frames as f32;
    assert!(
        (mean - 10.5).abs() < 0.15,
        "dithered mean should land on 10.5, got {mean}"
    );
}

#[test]
fn rgbw_subtract_moves_the_achromatic_part_into_white() {
    let (rgb, w) = split_white(Rgb::new(0.8, 0.5, 0.5), RgbwMode::Subtract);
    assert!((w - 0.5).abs() < 1e-6);
    assert!((rgb.r - 0.3).abs() < 1e-6);
    assert_eq!(rgb.g, 0.0);
    assert_eq!(rgb.b, 0.0);

    let (rgb, w) = split_white(Rgb::new(0.8, 0.5, 0.5), RgbwMode::Additive);
    assert!((w - 0.5).abs() < 1e-6);
    assert!((rgb.r - 0.8).abs() < 1e-6);
}

#[test]
fn rgbw_pipeline_emits_a_white_channel() {
    let mut settings = plain();
    settings.rgbw_mode = RgbwMode::Subtract;
    let mut p = ColorPipeline::new(settings);
    let frame = p.process(&[Rgb::WHITE], 1.0 / 60.0);
    assert!(frame.is_rgbw());
    assert_eq!(frame.white[0], 255);
    assert_eq!(frame.rgb[0], maslight_core::Rgb8::new(0, 0, 0));
}

#[test]
fn tone_mapping_keeps_hdr_highlights_from_clipping_to_white() {
    // Without tone mapping a 3.0 red clips to pure red.
    let clipped = Rgb::new(3.0, 0.5, 0.2).clamp01();
    assert_eq!(clipped.r, 1.0);

    let mapped = tone_map(Rgb::new(3.0, 0.5, 0.2), 4.0);
    assert!(
        mapped.r < 1.0,
        "highlight should stay under 1.0: {mapped:?}"
    );
    // Hue is preserved: the ratio between channels survives.
    let before = 0.5 / 3.0;
    let after = mapped.g / mapped.r;
    assert!(
        (before - after).abs() < 1e-4,
        "hue drifted: {before} vs {after}"
    );
}

#[test]
fn corrupt_settings_are_clamped_rather_than_producing_nans() {
    let mut settings = ColorSettings {
        brightness: f32::NAN,
        gamma: 0.0,
        saturation: -5.0,
        temperature_k: 1.0,
        ..Default::default()
    };
    settings.sanitise();
    assert!(settings.brightness.is_finite());
    assert!(settings.gamma >= 0.1);
    assert_eq!(settings.saturation, 0.0);
    assert_eq!(settings.temperature_k, 1000.0);

    let mut p = ColorPipeline::new(settings);
    let frame = p.process(&[Rgb::new(f32::NAN, 1.0, 0.0)], 1.0 / 60.0);
    // A NaN input becomes black rather than poisoning the frame.
    assert_eq!(frame.rgb[0], maslight_core::Rgb8::new(0, 0, 0));
}
