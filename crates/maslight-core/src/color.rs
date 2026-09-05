//! The colour pipeline.
//!
//! Input is one linear-light [`Rgb`] per LED (produced by the zone reducer).
//! Output is a display-ready [`LedFrame`]. Every stage is optional and every
//! knob is serialisable, so a profile fully describes the look.
//!
//! Stage order:
//! 1. HDR tone mapping        6. Temporal smoothing
//! 2. Saturation / vibrance   7. Power limiting
//! 3. White balance           8. RGBW extraction
//! 4. Brightness              9. Gamma + temporal dithering
//! 5. Luminance policy

use serde::{Deserialize, Serialize};

use crate::types::{LedFrame, Rgb, Rgb8};

/// sRGB electro-optical transfer function: encoded 0..1 -> linear light 0..1.
#[inline]
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Inverse of [`srgb_to_linear`].
#[inline]
pub fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// 256-entry lookup table mapping an 8-bit sRGB channel to linear light.
///
/// The CPU reducer walks millions of pixels per second; this table is the
/// difference between a `powf` per channel and a single load.
pub fn srgb_lut() -> [f32; 256] {
    let mut lut = [0.0f32; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        *slot = srgb_to_linear(i as f32 / 255.0);
    }
    lut
}

/// Approximate blackbody colour for a temperature in Kelvin, in **linear**
/// light, normalised so that 6500 K is exactly neutral.
pub fn kelvin_multipliers(kelvin: f32) -> [f32; 3] {
    let at = |k: f32| -> [f32; 3] {
        let t = k.clamp(1000.0, 40000.0) / 100.0;
        let red = if t <= 66.0 {
            255.0
        } else {
            329.698_73 * (t - 60.0).powf(-0.133_204_76)
        };
        let green = if t <= 66.0 {
            99.470_8 * t.ln() - 161.119_57
        } else {
            288.122_17 * (t - 60.0).powf(-0.075_514_85)
        };
        let blue = if t >= 66.0 {
            255.0
        } else if t <= 19.0 {
            0.0
        } else {
            138.517_73 * (t - 10.0).ln() - 305.044_8
        };
        [
            srgb_to_linear(red.clamp(0.0, 255.0) / 255.0),
            srgb_to_linear(green.clamp(0.0, 255.0) / 255.0),
            srgb_to_linear(blue.clamp(0.0, 255.0) / 255.0),
        ]
    };
    let target = at(kelvin);
    let neutral = at(6500.0);
    [
        target[0] / neutral[0].max(1e-6),
        target[1] / neutral[1].max(1e-6),
        target[2] / neutral[2].max(1e-6),
    ]
}

/// What the eye-care luminance threshold does to dark scenes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LuminancePolicy {
    /// Lift anything darker than the threshold up to it, so the strip never
    /// goes fully dark during a dark scene.
    #[default]
    MinimumLevel,
    /// Cut anything darker than the threshold to black, so near-black scenes
    /// do not glow.
    DeadZone,
}

/// How the white channel of an RGBW strip is driven.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RgbwMode {
    /// Strip is RGB only; no white channel is emitted.
    #[default]
    None,
    /// White carries the achromatic part, RGB carries the rest. Cooler, more
    /// efficient, and the usual choice for SK6812-RGBW.
    Subtract,
    /// White is driven alongside RGB without removing it. Brighter, less pure.
    Additive,
}

/// Every colour knob a profile can carry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ColorSettings {
    /// Master output scale, 0..=1.
    pub brightness: f32,
    /// Midtone shaping. 1.0 is a linear pass-through, which is physically
    /// correct for PWM LEDs; above 1.0 lifts midtones, below crushes them.
    pub gamma: f32,
    /// 1.0 keeps the saturation of the source, 0.0 is greyscale, >1 exaggerates.
    pub saturation: f32,
    /// Extra saturation applied only to already-dull colours, 0..=1.
    pub vibrance: f32,
    /// White point in Kelvin. 6500 is neutral.
    pub temperature_k: f32,
    /// Per-channel trim applied after the white point, for strip calibration.
    pub channel_gain: [f32; 3],
    /// Eye-care threshold in relative luminance, 0..=1.
    pub luminance_threshold: f32,
    pub luminance_policy: LuminancePolicy,
    /// Exponential smoothing time constant in milliseconds. 0 disables it.
    pub smoothing_ms: f32,
    /// Largest allowed change per channel per second, in 0..1 units. `None`
    /// leaves the smoothing filter alone; a value here makes hard cuts calmer
    /// without slowing slow fades.
    pub max_slew_per_s: Option<f32>,
    /// Temporal error diffusion, which removes 8-bit banding on slow fades.
    pub dithering: bool,
    /// Tone map values above 1.0 instead of clipping them.
    pub hdr_tone_map: bool,
    /// Input value that should map to full output when tone mapping.
    pub hdr_peak: f32,
    /// Hard current budget in amperes. `None` disables limiting.
    pub power_limit_a: Option<f32>,
    /// Full-on current of one colour channel of one LED, in milliamperes.
    /// 20 mA for WS2812B / SK6812.
    pub led_channel_ma: f32,
    pub rgbw_mode: RgbwMode,
}

impl Default for ColorSettings {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            gamma: 1.0,
            saturation: 1.0,
            vibrance: 0.0,
            temperature_k: 6500.0,
            channel_gain: [1.0, 1.0, 1.0],
            luminance_threshold: 0.0,
            luminance_policy: LuminancePolicy::MinimumLevel,
            smoothing_ms: 90.0,
            max_slew_per_s: None,
            dithering: true,
            hdr_tone_map: false,
            hdr_peak: 4.0,
            power_limit_a: None,
            led_channel_ma: 20.0,
            rgbw_mode: RgbwMode::None,
        }
    }
}

impl ColorSettings {
    /// Clamp every field into a sane range. Called on load so a hand-edited or
    /// corrupt profile can never make the pipeline produce NaNs.
    pub fn sanitise(&mut self) {
        self.brightness = finite_or(self.brightness, 1.0).clamp(0.0, 1.0);
        self.gamma = finite_or(self.gamma, 1.0).clamp(0.1, 5.0);
        self.saturation = finite_or(self.saturation, 1.0).clamp(0.0, 4.0);
        self.vibrance = finite_or(self.vibrance, 0.0).clamp(0.0, 1.0);
        self.temperature_k = finite_or(self.temperature_k, 6500.0).clamp(1000.0, 40000.0);
        for g in &mut self.channel_gain {
            *g = finite_or(*g, 1.0).clamp(0.0, 4.0);
        }
        self.luminance_threshold = finite_or(self.luminance_threshold, 0.0).clamp(0.0, 1.0);
        self.smoothing_ms = finite_or(self.smoothing_ms, 0.0).clamp(0.0, 5000.0);
        self.max_slew_per_s = self
            .max_slew_per_s
            .map(|s| finite_or(s, 1.0).clamp(0.01, 100.0));
        self.hdr_peak = finite_or(self.hdr_peak, 4.0).clamp(1.0, 100.0);
        self.power_limit_a = self
            .power_limit_a
            .map(|p| finite_or(p, 1.0).clamp(0.05, 1000.0));
        self.led_channel_ma = finite_or(self.led_channel_ma, 20.0).clamp(1.0, 100.0);
    }
}

#[inline]
fn finite_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

/// Stateful colour pipeline. One instance per output chain.
#[derive(Debug, Default)]
pub struct ColorPipeline {
    settings: ColorSettings,
    /// Smoothed linear colour, one per LED.
    state: Vec<Rgb>,
    /// Sub-LSB residual carried to the next frame, one per channel per LED.
    dither: Vec<[f32; 3]>,
    warm: bool,
}

impl ColorPipeline {
    pub fn new(settings: ColorSettings) -> Self {
        let mut settings = settings;
        settings.sanitise();
        Self {
            settings,
            state: Vec::new(),
            dither: Vec::new(),
            warm: false,
        }
    }

    pub fn settings(&self) -> &ColorSettings {
        &self.settings
    }

    /// Replace the settings. Smoothing state is kept, so dragging a slider
    /// does not make the strip jump.
    pub fn set_settings(&mut self, settings: ColorSettings) {
        let mut settings = settings;
        settings.sanitise();
        self.settings = settings;
    }

    /// Drop smoothing state so the next frame is shown immediately.
    pub fn reset(&mut self) {
        self.state.clear();
        self.dither.clear();
        self.warm = false;
    }

    /// Run one frame. `dt` is the time since the previous call, in seconds.
    pub fn process(&mut self, input: &[Rgb], dt: f32) -> LedFrame {
        let n = input.len();
        if self.state.len() != n {
            self.state = vec![Rgb::BLACK; n];
            self.dither = vec![[0.0; 3]; n];
            self.warm = false;
        }

        let s = self.settings.clone();
        let wb = kelvin_multipliers(s.temperature_k);
        let gain = [
            wb[0] * s.channel_gain[0],
            wb[1] * s.channel_gain[1],
            wb[2] * s.channel_gain[2],
        ];
        let alpha = smoothing_alpha(s.smoothing_ms, dt);
        let slew = s.max_slew_per_s.map(|v| v * dt.max(1e-4));

        let mut linear = Vec::with_capacity(n);
        for (i, raw) in input.iter().enumerate() {
            let mut c = if raw.is_finite() { *raw } else { Rgb::BLACK };

            c = if s.hdr_tone_map {
                tone_map(c, s.hdr_peak)
            } else {
                c.clamp01()
            };

            c = apply_saturation(c, s.saturation, s.vibrance);
            c = c.mul_channels(gain);
            c = c.scaled(s.brightness);
            c = apply_luminance_policy(c, s.luminance_threshold, s.luminance_policy);
            c = c.clamp01();

            // Temporal smoothing in linear light, frame-rate independent.
            let prev = self.state[i];
            let mut smoothed = if self.warm { prev.lerp(c, alpha) } else { c };
            if let Some(step) = slew {
                smoothed = Rgb::new(
                    clamp_step(prev.r, smoothed.r, step),
                    clamp_step(prev.g, smoothed.g, step),
                    clamp_step(prev.b, smoothed.b, step),
                );
            }
            self.state[i] = smoothed;
            linear.push(smoothed);
        }
        self.warm = true;

        // Power limiting runs on post-smoothing values so the limiter can never
        // fight the smoother.
        if let Some(limit_a) = s.power_limit_a {
            let scale = power_scale(&linear, limit_a, s.led_channel_ma, s.gamma);
            if scale < 1.0 {
                for c in &mut linear {
                    *c = c.scaled(scale);
                }
            }
        }

        let mut frame = LedFrame {
            rgb: Vec::with_capacity(n),
            white: match s.rgbw_mode {
                RgbwMode::None => Vec::new(),
                _ => Vec::with_capacity(n),
            },
        };

        let inv_gamma = 1.0 / s.gamma;
        for (i, c) in linear.iter().enumerate() {
            let (mut rgb, w) = split_white(*c, s.rgbw_mode);
            if (s.gamma - 1.0).abs() > 1e-4 {
                rgb = Rgb::new(
                    rgb.r.powf(inv_gamma),
                    rgb.g.powf(inv_gamma),
                    rgb.b.powf(inv_gamma),
                );
            }

            let residual = &mut self.dither[i];
            let out = if s.dithering {
                Rgb8::new(
                    quantise_dither(rgb.r, &mut residual[0]),
                    quantise_dither(rgb.g, &mut residual[1]),
                    quantise_dither(rgb.b, &mut residual[2]),
                )
            } else {
                Rgb8::new(quantise(rgb.r), quantise(rgb.g), quantise(rgb.b))
            };
            frame.rgb.push(out);
            if s.rgbw_mode != RgbwMode::None {
                let w = if (s.gamma - 1.0).abs() > 1e-4 {
                    w.powf(inv_gamma)
                } else {
                    w
                };
                frame.white.push(quantise(w));
            }
        }
        frame
    }
}

#[inline]
fn clamp_step(prev: f32, target: f32, step: f32) -> f32 {
    let d = target - prev;
    if d > step {
        prev + step
    } else if d < -step {
        prev - step
    } else {
        target
    }
}

/// `1 - e^(-dt/tau)`, the frame-rate independent one-pole coefficient.
#[inline]
pub fn smoothing_alpha(smoothing_ms: f32, dt: f32) -> f32 {
    if smoothing_ms <= 0.0 || dt <= 0.0 {
        return 1.0;
    }
    let tau = smoothing_ms / 1000.0;
    (1.0 - (-dt / tau).exp()).clamp(0.0, 1.0)
}

/// Extended Reinhard tone mapping. The scale is derived from the brightest
/// channel and applied to all three, so hue survives.
#[inline]
pub fn tone_map(c: Rgb, peak: f32) -> Rgb {
    let m = c.max_channel();
    if m <= 1.0 {
        return c.clamp01();
    }
    let w2 = (peak * peak).max(1e-6);
    let mapped = (m * (1.0 + m / w2)) / (1.0 + m);
    c.scaled(mapped / m).clamp01()
}

/// Saturation around luminance, plus vibrance which only lifts dull colours.
#[inline]
pub fn apply_saturation(c: Rgb, saturation: f32, vibrance: f32) -> Rgb {
    if (saturation - 1.0).abs() < 1e-4 && vibrance < 1e-4 {
        return c;
    }
    let l = c.luminance();
    let current = (c.max_channel() - c.min_channel()).clamp(0.0, 1.0);
    let s = saturation * (1.0 + vibrance * (1.0 - current));
    Rgb::new(l + (c.r - l) * s, l + (c.g - l) * s, l + (c.b - l) * s)
}

/// Eye-care floor or dead zone.
#[inline]
pub fn apply_luminance_policy(c: Rgb, threshold: f32, policy: LuminancePolicy) -> Rgb {
    if threshold <= 0.0 {
        return c;
    }
    let l = c.luminance();
    match policy {
        LuminancePolicy::DeadZone => {
            if l < threshold {
                Rgb::BLACK
            } else {
                c
            }
        }
        LuminancePolicy::MinimumLevel => {
            if l >= threshold {
                c
            } else if l <= 1e-5 {
                // Pure black has no hue to lift, so use a neutral floor.
                Rgb::splat(threshold)
            } else {
                c.scaled(threshold / l)
            }
        }
    }
}

/// Scale factor that keeps the whole strip inside a current budget.
fn power_scale(linear: &[Rgb], limit_a: f32, channel_ma: f32, gamma: f32) -> f32 {
    let inv_gamma = 1.0 / gamma;
    let duty: f32 = linear
        .iter()
        .map(|c| c.r.powf(inv_gamma) + c.g.powf(inv_gamma) + c.b.powf(inv_gamma))
        .sum();
    let amps = duty * channel_ma / 1000.0;
    if amps <= limit_a || amps <= 0.0 {
        1.0
    } else {
        // Duty is proportional to value^(1/gamma), so undo that when solving
        // for the linear-light scale.
        (limit_a / amps).powf(gamma)
    }
}

/// Split an RGB colour into RGB + white for an RGBW strip.
#[inline]
pub fn split_white(c: Rgb, mode: RgbwMode) -> (Rgb, f32) {
    match mode {
        RgbwMode::None => (c, 0.0),
        RgbwMode::Additive => (c, c.min_channel()),
        RgbwMode::Subtract => {
            let w = c.min_channel();
            (Rgb::new(c.r - w, c.g - w, c.b - w), w)
        }
    }
}

#[inline]
fn quantise(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Quantise with error feedback: the fraction we could not represent this
/// frame is carried into the next one, which turns 8-bit banding on a slow
/// fade into imperceptible temporal noise.
#[inline]
fn quantise_dither(v: f32, residual: &mut f32) -> u8 {
    let target = v.clamp(0.0, 1.0) * 255.0 + *residual;
    let out = target.round().clamp(0.0, 255.0);
    *residual = (target - out).clamp(-1.0, 1.0);
    out as u8
}
