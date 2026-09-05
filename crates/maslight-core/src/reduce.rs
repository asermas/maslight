//! Zone reduction: a captured frame in, one linear colour per LED out.
//!
//! This is the hot path. The cost is bounded by the *number of taps*, not by
//! the resolution of the frame: each LED rectangle is sampled on a grid of at
//! most `sample_grid` squared points, so a 4K frame and a 720p frame cost the
//! same. For 60 LEDs at a grid of 16 that is 15 360 taps per frame, which is
//! roughly 30 microseconds on one core.
//!
//! Averaging happens in **linear light** via a 256-entry lookup table. The
//! difference is visible: averaging sRGB-encoded values makes a half-black,
//! half-white screen produce a colour that is far too bright.

use crate::color::srgb_lut;
use crate::layout::Rect;
use crate::types::Rgb;

/// Memory layout of a captured frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// Windows Desktop Duplication and most X11 visuals.
    Bgra8,
    Rgba8,
    /// Half-float scRGB, which is what Desktop Duplication hands back while
    /// Windows HDR is on. Already linear, and values above 1.0 are the
    /// highlights that tone mapping exists for.
    Rgba16FScrgb,
}

impl PixelFormat {
    /// Bytes per pixel.
    #[inline]
    pub const fn bytes(self) -> usize {
        match self {
            PixelFormat::Bgra8 | PixelFormat::Rgba8 => 4,
            PixelFormat::Rgba16FScrgb => 8,
        }
    }

    /// True when values are already linear and may exceed 1.0.
    #[inline]
    pub const fn is_hdr(self) -> bool {
        matches!(self, PixelFormat::Rgba16FScrgb)
    }

    #[inline]
    const fn offsets(self) -> (usize, usize, usize) {
        match self {
            // (r, g, b) byte offsets inside a 4-byte pixel
            PixelFormat::Bgra8 => (2, 1, 0),
            PixelFormat::Rgba8 | PixelFormat::Rgba16FScrgb => (0, 1, 2),
        }
    }
}

/// Decode an IEEE 754 binary16 value.
///
/// Written out rather than pulled from a crate: it is eight lines, it is on the
/// hot path for HDR capture, and it means one less dependency to audit.
#[inline]
pub fn f16_to_f32(bits: u16) -> f32 {
    let sign = (bits >> 15) as u32;
    let exp = ((bits >> 10) & 0x1f) as u32;
    let frac = (bits & 0x03ff) as u32;
    let out = match exp {
        0 if frac == 0 => sign << 31,
        // Subnormals: renormalise into a binary32 exponent.
        0 => {
            let mut e = -1i32;
            let mut f = frac;
            while f & 0x0400 == 0 {
                f <<= 1;
                e -= 1;
            }
            let exp32 = (127 - 15 + e + 1) as u32;
            (sign << 31) | (exp32 << 23) | ((f & 0x03ff) << 13)
        }
        // Inf and NaN.
        31 => (sign << 31) | (0xff << 23) | (frac << 13),
        _ => (sign << 31) | ((exp + 127 - 15) << 23) | (frac << 13),
    };
    f32::from_bits(out)
}

/// A borrowed view of one captured frame.
#[derive(Clone, Copy, Debug)]
pub struct FrameView<'a> {
    pub data: &'a [u8],
    pub width: u32,
    pub height: u32,
    /// Bytes per row, which is often larger than `width * 4`.
    pub stride: usize,
    pub format: PixelFormat,
}

impl<'a> FrameView<'a> {
    pub fn new(
        data: &'a [u8],
        width: u32,
        height: u32,
        stride: usize,
        format: PixelFormat,
    ) -> Self {
        Self {
            data,
            width,
            height,
            stride,
            format,
        }
    }

    /// True when the buffer is at least as large as the geometry claims.
    pub fn is_valid(&self) -> bool {
        let bpp = self.format.bytes();
        self.width > 0
            && self.height > 0
            && self.stride >= self.width as usize * bpp
            && self.data.len()
                >= self.stride * (self.height as usize - 1) + self.width as usize * bpp
    }
}

/// Insets to apply before sampling, as fractions of the frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl Insets {
    pub fn is_zero(&self) -> bool {
        self.top <= 0.0 && self.bottom <= 0.0 && self.left <= 0.0 && self.right <= 0.0
    }

    /// Map a rectangle from full-frame space into the inset content area.
    pub fn apply(&self, r: Rect) -> Rect {
        if self.is_zero() {
            return r;
        }
        let w = (1.0 - self.left - self.right).max(0.05);
        let h = (1.0 - self.top - self.bottom).max(0.05);
        Rect {
            x: self.left + r.x * w,
            y: self.top + r.y * h,
            w: r.w * w,
            h: r.h * h,
        }
    }
}

/// Reusable reducer. Holds the sRGB table so it is built once per process.
#[derive(Clone)]
pub struct Reducer {
    lut: [f32; 256],
    grid: u32,
}

impl std::fmt::Debug for Reducer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reducer").field("grid", &self.grid).finish()
    }
}

impl Default for Reducer {
    fn default() -> Self {
        Self::new(16)
    }
}

impl Reducer {
    pub fn new(grid: u32) -> Self {
        Self {
            lut: srgb_lut(),
            grid: grid.clamp(2, 64),
        }
    }

    pub fn set_grid(&mut self, grid: u32) {
        self.grid = grid.clamp(2, 64);
    }

    pub fn grid(&self) -> u32 {
        self.grid
    }

    /// Average every rectangle. `out` is resized to `rects.len()`.
    pub fn reduce(
        &self,
        frame: &FrameView<'_>,
        rects: &[Rect],
        insets: Insets,
        out: &mut Vec<Rgb>,
    ) {
        out.clear();
        out.reserve(rects.len());
        if !frame.is_valid() {
            out.extend(std::iter::repeat_n(Rgb::BLACK, rects.len()));
            return;
        }
        for rect in rects {
            out.push(self.reduce_one(frame, insets.apply(*rect)));
        }
    }

    /// Average a single rectangle.
    pub fn reduce_one(&self, frame: &FrameView<'_>, rect: Rect) -> Rgb {
        let (x0, y0, x1, y1) = rect.to_pixels(frame.width, frame.height);
        let (ro, go, bo) = frame.format.offsets();
        let bpp = frame.format.bytes();
        let hdr = frame.format.is_hdr();

        let span_x = (x1 - x0).max(1);
        let span_y = (y1 - y0).max(1);
        let step_x = span_x.div_ceil(self.grid).max(1);
        let step_y = span_y.div_ceil(self.grid).max(1);

        let mut acc = [0.0f32; 3];
        let mut taps = 0u32;

        let mut y = y0;
        while y < y1 {
            let row = y as usize * frame.stride;
            let mut x = x0;
            while x < x1 {
                let p = row + x as usize * bpp;
                // `is_valid` guarantees the last full pixel of the last row is
                // inside the buffer, so this cannot go out of bounds.
                if p + bpp <= frame.data.len() {
                    if hdr {
                        acc[0] += half_at(frame.data, p + ro * 2);
                        acc[1] += half_at(frame.data, p + go * 2);
                        acc[2] += half_at(frame.data, p + bo * 2);
                    } else {
                        acc[0] += self.lut[frame.data[p + ro] as usize];
                        acc[1] += self.lut[frame.data[p + go] as usize];
                        acc[2] += self.lut[frame.data[p + bo] as usize];
                    }
                    taps += 1;
                }
                x += step_x;
            }
            y += step_y;
        }

        if taps == 0 {
            return Rgb::BLACK;
        }
        let inv = 1.0 / taps as f32;
        Rgb::new(acc[0] * inv, acc[1] * inv, acc[2] * inv)
    }
}

/// Read a half-float channel, clamping away negatives from the wide scRGB
/// gamut so they cannot pull an average below black.
#[inline]
fn half_at(data: &[u8], p: usize) -> f32 {
    let bits = u16::from_le_bytes([data[p], data[p + 1]]);
    let v = f16_to_f32(bits);
    if v.is_finite() {
        v.max(0.0)
    } else {
        0.0
    }
}

/// Letterbox detection.
///
/// Walks a sparse set of rows and columns from each edge inwards and reports
/// how much of the frame is black bar. The result is deliberately quantised to
/// avoid the sample rectangles twitching by a pixel every frame.
pub fn detect_black_bars(frame: &FrameView<'_>, threshold: f32) -> Insets {
    if !frame.is_valid() {
        return Insets::default();
    }
    let lut = srgb_lut();
    let (ro, go, bo) = frame.format.offsets();
    let bpp = frame.format.bytes();
    let hdr = frame.format.is_hdr();
    let w = frame.width;
    let h = frame.height;
    // Sample at most 64 points across a line; bars are uniform, so this is
    // plenty and keeps the scan cheap.
    let step_x = (w / 64).max(1);
    let step_y = (h / 64).max(1);

    let luminance_at = |p: usize| -> f32 {
        if p + bpp > frame.data.len() {
            return 0.0;
        }
        let (r, g, b) = if hdr {
            (
                half_at(frame.data, p + ro * 2),
                half_at(frame.data, p + go * 2),
                half_at(frame.data, p + bo * 2),
            )
        } else {
            (
                lut[frame.data[p + ro] as usize],
                lut[frame.data[p + go] as usize],
                lut[frame.data[p + bo] as usize],
            )
        };
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };

    let row_is_dark = |y: u32| -> bool {
        let row = y as usize * frame.stride;
        let mut x = 0;
        while x < w {
            if luminance_at(row + x as usize * bpp) > threshold {
                return false;
            }
            x += step_x;
        }
        true
    };
    let col_is_dark = |x: u32| -> bool {
        let mut y = 0;
        while y < h {
            if luminance_at(y as usize * frame.stride + x as usize * bpp) > threshold {
                return false;
            }
            y += step_y;
        }
        true
    };

    // Never eat more than 40% from any side: that is a broken frame, not a bar.
    let limit_y = (h as f32 * 0.4) as u32;
    let limit_x = (w as f32 * 0.4) as u32;

    let mut top = 0;
    while top < limit_y && row_is_dark(top) {
        top += 1;
    }
    let mut bottom = 0;
    while bottom < limit_y && row_is_dark(h - 1 - bottom) {
        bottom += 1;
    }
    let mut left = 0;
    while left < limit_x && col_is_dark(left) {
        left += 1;
    }
    let mut right = 0;
    while right < limit_x && col_is_dark(w - 1 - right) {
        right += 1;
    }

    // Quantise to 0.5% so a single noisy pixel row does not move the bars.
    let q = |v: u32, total: u32| -> f32 {
        let f = v as f32 / total as f32;
        if f < 0.01 {
            0.0
        } else {
            (f * 200.0).floor() / 200.0
        }
    };
    Insets {
        top: q(top, h),
        bottom: q(bottom, h),
        left: q(left, w),
        right: q(right, w),
    }
}
