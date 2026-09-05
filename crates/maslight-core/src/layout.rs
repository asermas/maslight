//! The LED layout model.
//!
//! A layout says, for every LED in the physical chain, which display it looks
//! at and which rectangle of that display it samples. Coordinates are
//! normalised per display (`0.0..=1.0`, origin top-left), so a resolution
//! change never invalidates a layout.
//!
//! The wizard in [`WizardParams`] generates a layout from four edge counts;
//! the canvas editor then edits individual LEDs freely.

use serde::{Deserialize, Serialize};

use crate::types::ColorOrder;

/// Current on-disk layout version. Bumped only on a breaking change.
pub const LAYOUT_VERSION: u32 = 1;

/// A normalised rectangle inside one display.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge, 0..1.
    pub x: f32,
    /// Top edge, 0..1.
    pub y: f32,
    /// Width, 0..1.
    pub w: f32,
    /// Height, 0..1.
    pub h: f32,
}

impl Rect {
    pub const FULL: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Build a rect from its centre and size.
    pub fn centred(cx: f32, cy: f32, w: f32, h: f32) -> Self {
        Self::new(cx - w * 0.5, cy - h * 0.5, w, h)
    }

    pub fn centre(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    /// Clamp the rect inside the display, keeping at least one pixel of size.
    pub fn clamped(&self) -> Rect {
        let w = self.w.clamp(0.001, 1.0);
        let h = self.h.clamp(0.001, 1.0);
        Rect {
            x: self.x.clamp(0.0, 1.0 - w),
            y: self.y.clamp(0.0, 1.0 - h),
            w,
            h,
        }
    }

    /// Convert to integer pixel bounds `(x0, y0, x1, y1)` for a given surface.
    /// The result is always at least one pixel wide and tall.
    pub fn to_pixels(&self, width: u32, height: u32) -> (u32, u32, u32, u32) {
        let r = self.clamped();
        let x0 = (r.x * width as f32).floor().max(0.0) as u32;
        let y0 = (r.y * height as f32).floor().max(0.0) as u32;
        let x1 = ((r.x + r.w) * width as f32).ceil().min(width as f32) as u32;
        let y1 = ((r.y + r.h) * height as f32).ceil().min(height as f32) as u32;
        (
            x0.min(width.saturating_sub(1)),
            y0.min(height.saturating_sub(1)),
            x1.max(x0 + 1).min(width),
            y1.max(y0 + 1).min(height),
        )
    }
}

/// One physical display that LEDs can sample from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayRegion {
    /// Stable platform identifier, e.g. `\\.\DISPLAY1` or an X11 output name.
    pub id: String,
    /// Human label shown in the UI.
    #[serde(default)]
    pub label: String,
    /// Virtual-desktop bounds in pixels: `[x, y, width, height]`. Informational;
    /// sampling always uses normalised coordinates.
    #[serde(default)]
    pub bounds: [i32; 4],
}

impl DisplayRegion {
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            label: id.clone(),
            id,
            bounds: [0, 0, 0, 0],
        }
    }
}

/// One LED in the physical chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedSpec {
    /// Position in the physical chain, starting at 0. Always contiguous: a
    /// physically absent or intentionally dark LED stays in the list with
    /// `enabled: false` so the wire order is never disturbed.
    pub index: u32,
    /// Which display this LED samples. Empty means "effect only": the LED is
    /// driven by effects but ignores the screen.
    #[serde(default)]
    pub display: String,
    /// Sampling rectangle inside that display.
    pub rect: Rect,
    /// Relative weight when averaging, mostly used to fade corner LEDs.
    #[serde(default = "one")]
    pub weight: f32,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

impl LedSpec {
    pub fn new(index: u32, display: impl Into<String>, rect: Rect) -> Self {
        Self {
            index,
            display: display.into(),
            rect,
            weight: 1.0,
            enabled: true,
        }
    }

    /// True when this LED takes its colour from the screen.
    pub fn samples_screen(&self) -> bool {
        self.enabled && !self.display.is_empty() && self.weight > 0.0
    }
}

/// Wiring properties of the chain as a whole.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChainConfig {
    pub color_order: ColorOrder,
    /// Strip has a dedicated white channel (SK6812-RGBW and friends).
    pub rgbw: bool,
    /// Send the chain back to front. Saves rewiring an upside-down strip.
    pub reverse: bool,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            color_order: ColorOrder::Grb,
            rgbw: false,
            reverse: false,
        }
    }
}

/// A complete LED layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    #[serde(default = "layout_version")]
    pub version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub displays: Vec<DisplayRegion>,
    #[serde(default)]
    pub chain: ChainConfig,
    #[serde(default)]
    pub leds: Vec<LedSpec>,
}

fn layout_version() -> u32 {
    LAYOUT_VERSION
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            version: LAYOUT_VERSION,
            name: String::new(),
            displays: Vec::new(),
            chain: ChainConfig::default(),
            leds: Vec::new(),
        }
    }
}

impl Layout {
    pub fn len(&self) -> usize {
        self.leds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leds.is_empty()
    }

    /// Every LED that samples the given display, in chain order.
    pub fn leds_for_display<'a>(&'a self, display: &'a str) -> impl Iterator<Item = &'a LedSpec> {
        self.leds
            .iter()
            .filter(move |l| l.samples_screen() && l.display == display)
    }

    /// Display ids referenced by at least one enabled LED.
    pub fn active_displays(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for led in self.leds.iter().filter(|l| l.samples_screen()) {
            if !out.iter().any(|d| d == &led.display) {
                out.push(led.display.clone());
            }
        }
        out
    }

    /// Re-index LEDs so the chain is contiguous, clamp rects and weights, and
    /// make sure every referenced display exists.
    pub fn normalise(&mut self) {
        if self.version == 0 {
            self.version = LAYOUT_VERSION;
        }
        for (i, led) in self.leds.iter_mut().enumerate() {
            led.index = i as u32;
            led.rect = led.rect.clamped();
            if !led.weight.is_finite() {
                led.weight = 1.0;
            }
            led.weight = led.weight.clamp(0.0, 10.0);
        }
        let referenced: Vec<String> = self
            .leds
            .iter()
            .filter(|l| !l.display.is_empty())
            .map(|l| l.display.clone())
            .collect();
        for id in referenced {
            if !self.displays.iter().any(|d| d.id == id) {
                self.displays.push(DisplayRegion::new(id));
            }
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        let mut layout: Layout = serde_json::from_str(s)?;
        layout.normalise();
        Ok(layout)
    }

    /// Build a layout from wizard parameters.
    pub fn from_wizard(params: &WizardParams) -> Layout {
        let mut leds = params.generate();
        if params.chain_offset != 0 && !leds.is_empty() {
            let n = leds.len() as i32;
            let shift = ((-params.chain_offset % n) + n) % n;
            leds.rotate_left(shift as usize);
        }
        if params.reverse_chain {
            leds.reverse();
        }
        for (i, led) in leds.iter_mut().enumerate() {
            led.index = i as u32;
        }
        let mut layout = Layout {
            version: LAYOUT_VERSION,
            name: params.name.clone(),
            displays: vec![DisplayRegion::new(params.display.clone())],
            chain: params.chain.clone(),
            leds,
        };
        layout.normalise();
        layout
    }
}

/// Which physical corner the first LED of the chain sits in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    #[default]
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}

/// Which way the chain runs when seen from the front of the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    Clockwise,
    #[default]
    CounterClockwise,
}

/// LED counts along each edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EdgeCounts {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

impl EdgeCounts {
    pub fn total(&self) -> u32 {
        self.top + self.right + self.bottom + self.left
    }
}

/// Everything the setup wizard needs to lay out a conventional strip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WizardParams {
    pub name: String,
    /// Display id the generated LEDs sample.
    pub display: String,
    pub counts: EdgeCounts,
    pub start_corner: Corner,
    pub direction: Direction,
    /// How far into the screen each sample rectangle reaches, 0..0.5.
    pub depth: f32,
    /// How much of each edge the strip covers, 0..1, centred.
    pub span: f32,
    /// Rotate the finished chain by this many LEDs. Use it when the strip
    /// starts halfway along an edge instead of in a corner.
    pub chain_offset: i32,
    /// Reverse the finished chain.
    pub reverse_chain: bool,
    pub chain: ChainConfig,
}

impl Default for WizardParams {
    fn default() -> Self {
        Self {
            name: String::from("Screen"),
            display: String::new(),
            counts: EdgeCounts {
                top: 0,
                right: 0,
                bottom: 0,
                left: 0,
            },
            start_corner: Corner::BottomLeft,
            direction: Direction::CounterClockwise,
            depth: 0.12,
            span: 1.0,
            chain_offset: 0,
            reverse_chain: false,
            chain: ChainConfig::default(),
        }
    }
}

/// One of the four screen edges, plus the travel direction along it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    /// Left to right along the bottom.
    BottomLtr,
    /// Right to left along the bottom.
    BottomRtl,
    /// Left to right along the top.
    TopLtr,
    TopRtl,
    /// Bottom to top along the left edge.
    LeftBtt,
    LeftTtb,
    RightBtt,
    RightTtb,
}

impl WizardParams {
    /// Generate LED specs in chain order, before offset and reversal.
    fn generate(&self) -> Vec<LedSpec> {
        let depth = self.depth.clamp(0.005, 0.5);
        let span = self.span.clamp(0.05, 1.0);
        let margin = (1.0 - span) * 0.5;

        let mut leds = Vec::with_capacity(self.counts.total() as usize);
        for edge in self.edge_order() {
            let count = match edge {
                Edge::BottomLtr | Edge::BottomRtl => self.counts.bottom,
                Edge::TopLtr | Edge::TopRtl => self.counts.top,
                Edge::LeftBtt | Edge::LeftTtb => self.counts.left,
                Edge::RightBtt | Edge::RightTtb => self.counts.right,
            };
            if count == 0 {
                continue;
            }
            let step = span / count as f32;
            for i in 0..count {
                // Position along the edge, 0..1, in travel direction.
                let t = margin + step * (i as f32 + 0.5);
                let rect = match edge {
                    Edge::BottomLtr => Rect::centred(t, 1.0 - depth * 0.5, step, depth),
                    Edge::BottomRtl => Rect::centred(1.0 - t, 1.0 - depth * 0.5, step, depth),
                    Edge::TopLtr => Rect::centred(t, depth * 0.5, step, depth),
                    Edge::TopRtl => Rect::centred(1.0 - t, depth * 0.5, step, depth),
                    Edge::LeftBtt => Rect::centred(depth * 0.5, 1.0 - t, depth, step),
                    Edge::LeftTtb => Rect::centred(depth * 0.5, t, depth, step),
                    Edge::RightBtt => Rect::centred(1.0 - depth * 0.5, 1.0 - t, depth, step),
                    Edge::RightTtb => Rect::centred(1.0 - depth * 0.5, t, depth, step),
                };
                leds.push(LedSpec::new(
                    leds.len() as u32,
                    self.display.clone(),
                    rect.clamped(),
                ));
            }
        }
        leds
    }

    /// Walk the four edges from the start corner in the chosen direction.
    fn edge_order(&self) -> [Edge; 4] {
        use Corner::*;
        use Direction::*;
        match (self.start_corner, self.direction) {
            // Counter-clockwise as seen by the viewer: bottom-left -> right
            // along the bottom -> up the right edge -> back along the top.
            (BottomLeft, CounterClockwise) => {
                [Edge::BottomLtr, Edge::RightBtt, Edge::TopRtl, Edge::LeftTtb]
            }
            (BottomRight, CounterClockwise) => {
                [Edge::RightBtt, Edge::TopRtl, Edge::LeftTtb, Edge::BottomLtr]
            }
            (TopRight, CounterClockwise) => {
                [Edge::TopRtl, Edge::LeftTtb, Edge::BottomLtr, Edge::RightBtt]
            }
            (TopLeft, CounterClockwise) => {
                [Edge::LeftTtb, Edge::BottomLtr, Edge::RightBtt, Edge::TopRtl]
            }
            (BottomLeft, Clockwise) => {
                [Edge::LeftBtt, Edge::TopLtr, Edge::RightTtb, Edge::BottomRtl]
            }
            (TopLeft, Clockwise) => [Edge::TopLtr, Edge::RightTtb, Edge::BottomRtl, Edge::LeftBtt],
            (TopRight, Clockwise) => [Edge::RightTtb, Edge::BottomRtl, Edge::LeftBtt, Edge::TopLtr],
            (BottomRight, Clockwise) => {
                [Edge::BottomRtl, Edge::LeftBtt, Edge::TopLtr, Edge::RightTtb]
            }
        }
    }
}
