//! Turning a spectrum into colours on a strip.
//!
//! Each effect maps the analysis onto the chain in linear light, because that
//! is what the colour pipeline downstream expects. Nothing here knows about
//! gamma, brightness or white balance: those are applied once, later, for
//! every source.

use maslight_core::audio::{AudioEffect, Palette};
use maslight_core::Rgb;

use crate::analysis::Spectrum;

/// Render one frame of `len` LEDs.
pub fn render(
    effect: AudioEffect,
    spectrum: &Spectrum,
    palette: &Palette,
    beat_boost: f32,
    phase: f32,
    len: usize,
    out: &mut Vec<Rgb>,
) {
    out.clear();
    out.reserve(len);
    if len == 0 || spectrum.bands.is_empty() {
        out.extend(std::iter::repeat_n(Rgb::BLACK, len));
        return;
    }

    let pulse = 1.0 + beat_boost * spectrum.beat_envelope;

    match effect {
        AudioEffect::Spectrum => {
            for i in 0..len {
                // Mirror around the middle so both halves of the strip show
                // the same spectrum growing outwards.
                let t = i as f32 / len.max(1) as f32;
                let mirrored = if t < 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
                let level = sample_bands(&spectrum.bands, mirrored);
                let hue = lerp_hue(palette.from_hue, palette.to_hue, mirrored);
                out.push(hsv(hue, palette.saturation, (level * pulse).min(1.0)));
            }
        }
        AudioEffect::Energy => {
            let value = (spectrum.energy * pulse).min(1.0);
            let hue = lerp_hue(palette.from_hue, palette.to_hue, spectrum.energy);
            let colour = hsv(hue, palette.saturation, value);
            out.extend(std::iter::repeat_n(colour, len));
        }
        AudioEffect::Wave => {
            for i in 0..len {
                let t = i as f32 / len.max(1) as f32;
                let distance = (t - 0.5).abs() * 2.0;
                // The wave front moves outwards on every beat and fades as it
                // goes, so a quiet passage leaves the strip calm.
                let front = (spectrum.beat_envelope * 1.4 - distance).clamp(0.0, 1.0);
                let level = (spectrum.energy * 0.5 + front).min(1.0);
                let hue = lerp_hue(palette.from_hue, palette.to_hue, distance);
                out.push(hsv(hue, palette.saturation, level));
            }
        }
        AudioEffect::Scroll => {
            for i in 0..len {
                let t = i as f32 / len.max(1) as f32;
                let hue = (phase + t).fract();
                let level = (spectrum.energy * pulse).min(1.0);
                out.push(hsv(hue, palette.saturation, level));
            }
        }
    }
}

/// Read the band array at a fractional position, interpolating between bands.
fn sample_bands(bands: &[f32], t: f32) -> f32 {
    if bands.is_empty() {
        return 0.0;
    }
    let x = (t.clamp(0.0, 1.0) * (bands.len() - 1) as f32).max(0.0);
    let i = x.floor() as usize;
    let f = x - i as f32;
    let a = bands[i.min(bands.len() - 1)];
    let b = bands[(i + 1).min(bands.len() - 1)];
    a + (b - a) * f
}

/// Interpolate hue the short way round the wheel.
fn lerp_hue(from: f32, to: f32, t: f32) -> f32 {
    let mut delta = to - from;
    if delta > 0.5 {
        delta -= 1.0;
    } else if delta < -0.5 {
        delta += 1.0;
    }
    (from + delta * t.clamp(0.0, 1.0)).rem_euclid(1.0)
}

/// HSV to linear light RGB.
///
/// The value is treated as linear rather than as an sRGB code, because
/// everything downstream of here works in linear light.
pub fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h = h.rem_euclid(1.0) * 6.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i.rem_euclid(6) {
        0 => Rgb::new(v, t, p),
        1 => Rgb::new(q, v, p),
        2 => Rgb::new(p, v, t),
        3 => Rgb::new(p, q, v),
        4 => Rgb::new(t, p, v),
        _ => Rgb::new(v, p, q),
    }
}
