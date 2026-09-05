//! Core colour and frame types.
//!
//! Everything inside the pipeline is **linear light**. Values are normally in
//! `0.0..=1.0`, but HDR sources may exceed `1.0` until tone mapping runs.

use serde::{Deserialize, Serialize};

/// A linear-light RGB triple.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb {
        r: 0.0,
        g: 0.0,
        b: 0.0,
    };
    pub const WHITE: Rgb = Rgb {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    };

    #[inline]
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    #[inline]
    pub const fn splat(v: f32) -> Self {
        Self { r: v, g: v, b: v }
    }

    /// Rec.709 relative luminance, the same primaries sRGB uses.
    #[inline]
    pub fn luminance(&self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    #[inline]
    pub fn max_channel(&self) -> f32 {
        self.r.max(self.g).max(self.b)
    }

    #[inline]
    pub fn min_channel(&self) -> f32 {
        self.r.min(self.g).min(self.b)
    }

    #[inline]
    pub fn scaled(&self, k: f32) -> Self {
        Self::new(self.r * k, self.g * k, self.b * k)
    }

    #[inline]
    pub fn mul_channels(&self, k: [f32; 3]) -> Self {
        Self::new(self.r * k[0], self.g * k[1], self.b * k[2])
    }

    #[inline]
    pub fn lerp(&self, other: Rgb, t: f32) -> Self {
        Self::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
        )
    }

    #[inline]
    pub fn clamp01(&self) -> Self {
        Self::new(
            self.r.clamp(0.0, 1.0),
            self.g.clamp(0.0, 1.0),
            self.b.clamp(0.0, 1.0),
        )
    }

    #[inline]
    pub fn is_finite(&self) -> bool {
        self.r.is_finite() && self.g.is_finite() && self.b.is_finite()
    }

    /// Largest absolute per-channel difference — used by adaptive frame skipping.
    #[inline]
    pub fn max_delta(&self, other: Rgb) -> f32 {
        (self.r - other.r)
            .abs()
            .max((self.g - other.g).abs())
            .max((self.b - other.b).abs())
    }
}

/// An 8-bit display-ready RGB triple.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub const BLACK: Rgb8 = Rgb8 { r: 0, g: 0, b: 0 };

    #[inline]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Reorder the channels for a strip whose driver expects a different order.
    #[inline]
    pub fn to_order(self, order: ColorOrder) -> [u8; 3] {
        let Rgb8 { r, g, b } = self;
        match order {
            ColorOrder::Rgb => [r, g, b],
            ColorOrder::Rbg => [r, b, g],
            ColorOrder::Grb => [g, r, b],
            ColorOrder::Gbr => [g, b, r],
            ColorOrder::Brg => [b, r, g],
            ColorOrder::Bgr => [b, g, r],
        }
    }
}

/// Physical channel order of the LED chip.
///
/// WS2812B is `GRB`, WS2811 is usually `RGB`, APA102 is `BGR`. The calibration
/// wizard detects this automatically, but it can always be set by hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorOrder {
    Rgb,
    Rbg,
    #[default]
    Grb,
    Gbr,
    Brg,
    Bgr,
}

impl ColorOrder {
    pub fn as_str(&self) -> &'static str {
        match self {
            ColorOrder::Rgb => "RGB",
            ColorOrder::Rbg => "RBG",
            ColorOrder::Grb => "GRB",
            ColorOrder::Gbr => "GBR",
            ColorOrder::Brg => "BRG",
            ColorOrder::Bgr => "BGR",
        }
    }
}

/// One finished frame of LED data, ready for a [`Sink`](../../maslight_output).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LedFrame {
    /// One entry per LED in chain order.
    pub rgb: Vec<Rgb8>,
    /// White channel, one entry per LED. Empty when the strip is not RGBW.
    pub white: Vec<u8>,
}

impl LedFrame {
    pub fn black(len: usize) -> Self {
        Self {
            rgb: vec![Rgb8::BLACK; len],
            white: Vec::new(),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.rgb.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rgb.is_empty()
    }

    #[inline]
    pub fn is_rgbw(&self) -> bool {
        !self.white.is_empty()
    }

    /// Estimated current draw in amperes for the whole strip.
    ///
    /// `per_channel_ma` is the full-on current of a single colour channel —
    /// 20 mA for WS2812B, so 60 mA for a white pixel.
    pub fn estimated_current_a(&self, per_channel_ma: f32) -> f32 {
        let sum: f32 = self
            .rgb
            .iter()
            .map(|c| (c.r as f32 + c.g as f32 + c.b as f32) / 255.0)
            .sum::<f32>()
            + self.white.iter().map(|w| *w as f32 / 255.0).sum::<f32>();
        sum * per_channel_ma / 1000.0
    }
}
