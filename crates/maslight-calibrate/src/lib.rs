//! # maslight-calibrate
//!
//! Finding out where each LED physically is, with a camera.
//!
//! Every ambilight tool asks you to describe your strip: how many LEDs on the
//! top, which corner it starts in, which way it runs. That is a translation
//! job, and people get it wrong, because the answer depends on where you were
//! standing when you wound the strip on.
//!
//! This asks the strip instead. MasLight lights the LEDs in a sequence of
//! patterns, you photograph each pattern from one fixed spot, and the position
//! and the chain order both fall out of the photographs.
//!
//! ## How the sequence works
//!
//! With `n` LEDs, `ceil(log2(n + 1))` patterns are enough. In pattern `k`,
//! every LED whose **index plus one** has bit `k` set is lit. Reading the bits
//! back for one LED spells out its index.
//!
//! Index plus one rather than index, because an LED that is dark in every
//! pattern would otherwise be indistinguishable from an LED the camera never
//! saw at all.
//!
//! Two more frames bracket the sequence: one with everything off, one with
//! everything on. Their difference is where the LEDs are; every other frame
//! only has to answer "is this one lit".
//!
//! ## What comes out
//!
//! Positions in the photograph, and then, once the screen corners are known,
//! positions in the screen's own coordinates through a homography. That is the
//! space layouts are stored in, so the result drops straight into a layout
//! without anyone having to describe anything.

use maslight_core::{Layout, LedSpec, Rect};

mod homography;

pub use homography::Homography;

/// A single greyscale frame from the camera.
///
/// Greyscale on purpose: the sequence only ever asks whether a spot is lit,
/// and colour would make the caller convert twice.
#[derive(Clone, Debug)]
pub struct Frame {
    pub luma: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Frame {
    pub fn new(luma: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            luma,
            width,
            height,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.luma.len() >= (self.width as usize * self.height as usize)
    }

    #[inline]
    pub fn at(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.luma[y as usize * self.width as usize + x as usize]
    }

    /// Mean brightness inside a small square, which is what a blob test wants:
    /// a single pixel is noise, a patch is a reading.
    pub fn patch_mean(&self, cx: f32, cy: f32, radius: u32) -> f32 {
        let r = radius.max(1) as i64;
        let cx = cx.round() as i64;
        let cy = cy.round() as i64;
        let mut sum = 0u32;
        let mut count = 0u32;
        for y in (cy - r)..=(cy + r) {
            for x in (cx - r)..=(cx + r) {
                if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
                    continue;
                }
                sum += self.at(x as u32, y as u32) as u32;
                count += 1;
            }
        }
        if count == 0 {
            0.0
        } else {
            sum as f32 / count as f32
        }
    }
}

/// One step of the capture sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Everything dark. The reference the other frames are measured against.
    AllOff,
    /// Everything lit. Used to find where the LEDs are at all.
    AllOn,
    /// Only the LEDs whose index plus one has this bit set.
    Bit(u32),
}

impl Step {
    /// Whether this LED is lit in this step.
    pub fn lights(&self, index: usize) -> bool {
        match self {
            Step::AllOff => false,
            Step::AllOn => true,
            Step::Bit(bit) => ((index + 1) >> bit) & 1 == 1,
        }
    }
}

/// The sequence to photograph, in order.
pub fn plan(led_count: usize) -> Vec<Step> {
    let mut steps = vec![Step::AllOff, Step::AllOn];
    for bit in 0..bits_needed(led_count) {
        steps.push(Step::Bit(bit));
    }
    steps
}

/// Patterns needed to number `led_count` LEDs.
pub fn bits_needed(led_count: usize) -> u32 {
    let highest = led_count as u64; // the largest value encoded is led_count
    if highest == 0 {
        return 0;
    }
    64 - highest.leading_zeros()
}

/// A lit spot found in a photograph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blob {
    pub x: f32,
    pub y: f32,
    /// Pixels in the blob, used to drop specks and reflections.
    pub area: u32,
    /// Peak brightness of the difference, for ranking.
    pub peak: u8,
}

/// Everything the discovery produced.
#[derive(Clone, Debug, Default)]
pub struct Discovery {
    /// One entry per LED index, `None` where the camera never saw it.
    pub positions: Vec<Option<(f32, f32)>>,
    /// Blobs that were found but could not be numbered.
    pub unassigned: Vec<Blob>,
    pub found: usize,
    pub expected: usize,
}

impl Discovery {
    /// Fraction of the strip that was located.
    pub fn coverage(&self) -> f32 {
        if self.expected == 0 {
            return 0.0;
        }
        self.found as f32 / self.expected as f32
    }
}

/// Failures the discovery can report.
#[derive(Debug, thiserror::Error)]
pub enum CalibrateError {
    #[error("expected {expected} frames, got {got}")]
    FrameCount { expected: usize, got: usize },
    #[error("the frames are not all the same size")]
    SizeMismatch,
    #[error("a frame is empty or malformed")]
    BadFrame,
    #[error("no lit spots were found: check the exposure and that the strip was on")]
    NothingFound,
}

/// Tuning for the detector.
#[derive(Clone, Copy, Debug)]
pub struct DetectSettings {
    /// How much brighter than the dark reference a pixel has to be.
    pub threshold: u8,
    /// Smallest blob to keep, in pixels.
    pub min_area: u32,
    /// Largest blob to keep. A blob this big is a reflection or a window.
    pub max_area: u32,
    /// Radius of the patch each bit frame is read over.
    pub sample_radius: u32,
}

impl Default for DetectSettings {
    fn default() -> Self {
        Self {
            threshold: 40,
            min_area: 4,
            max_area: 20_000,
            sample_radius: 2,
        }
    }
}

/// Find the lit spots: everything that got brighter between the dark frame and
/// the lit frame.
pub fn detect_blobs(
    off: &Frame,
    on: &Frame,
    settings: &DetectSettings,
) -> Result<Vec<Blob>, CalibrateError> {
    if !off.is_valid() || !on.is_valid() {
        return Err(CalibrateError::BadFrame);
    }
    if off.width != on.width || off.height != on.height {
        return Err(CalibrateError::SizeMismatch);
    }

    let w = on.width as usize;
    let h = on.height as usize;
    let mut mask = vec![false; w * h];
    for (i, slot) in mask.iter_mut().enumerate() {
        let diff = on.luma[i].saturating_sub(off.luma[i]);
        *slot = diff >= settings.threshold;
    }

    // Connected components, four way, with an explicit stack. Recursion would
    // blow up on a large bright region and the whole point is to survive one.
    let mut visited = vec![false; w * h];
    let mut blobs = Vec::new();
    let mut stack: Vec<usize> = Vec::new();

    for start in 0..w * h {
        if !mask[start] || visited[start] {
            continue;
        }
        stack.clear();
        stack.push(start);
        visited[start] = true;

        let mut sum_x = 0f64;
        let mut sum_y = 0f64;
        let mut area = 0u32;
        let mut peak = 0u8;

        while let Some(p) = stack.pop() {
            let x = p % w;
            let y = p / w;
            area += 1;
            sum_x += x as f64;
            sum_y += y as f64;
            peak = peak.max(on.luma[p].saturating_sub(off.luma[p]));

            let mut push = |q: usize, stack: &mut Vec<usize>| {
                if mask[q] && !visited[q] {
                    visited[q] = true;
                    stack.push(q);
                }
            };
            if x > 0 {
                push(p - 1, &mut stack);
            }
            if x + 1 < w {
                push(p + 1, &mut stack);
            }
            if y > 0 {
                push(p - w, &mut stack);
            }
            if y + 1 < h {
                push(p + w, &mut stack);
            }
        }

        if area < settings.min_area || area > settings.max_area {
            continue;
        }
        blobs.push(Blob {
            x: (sum_x / area as f64) as f32,
            y: (sum_y / area as f64) as f32,
            area,
            peak,
        });
    }

    if blobs.is_empty() {
        return Err(CalibrateError::NothingFound);
    }
    Ok(blobs)
}

/// Read the bit frames and give every blob its place in the chain.
///
/// `frames` must be the photographs of [`plan`], in the same order.
pub fn discover(
    led_count: usize,
    frames: &[Frame],
    settings: &DetectSettings,
) -> Result<Discovery, CalibrateError> {
    let steps = plan(led_count);
    if frames.len() != steps.len() {
        return Err(CalibrateError::FrameCount {
            expected: steps.len(),
            got: frames.len(),
        });
    }

    let off = &frames[0];
    let on = &frames[1];
    let blobs = detect_blobs(off, on, settings)?;

    let mut positions: Vec<Option<(f32, f32)>> = vec![None; led_count];
    let mut unassigned = Vec::new();
    let mut found = 0usize;

    for blob in blobs {
        // The lit reference tells us how bright this spot is when it is on, so
        // the decision threshold follows the spot rather than the room.
        let dark = off.patch_mean(blob.x, blob.y, settings.sample_radius);
        let lit = on.patch_mean(blob.x, blob.y, settings.sample_radius);
        if lit - dark < settings.threshold as f32 * 0.5 {
            unassigned.push(blob);
            continue;
        }
        let midpoint = dark + (lit - dark) * 0.5;

        let mut value = 0usize;
        for (bit, frame) in frames[2..].iter().enumerate() {
            if frame.width != on.width || frame.height != on.height {
                return Err(CalibrateError::SizeMismatch);
            }
            if frame.patch_mean(blob.x, blob.y, settings.sample_radius) > midpoint {
                value |= 1 << bit;
            }
        }

        // The code carries index plus one, so zero means nothing was read.
        if value == 0 || value > led_count {
            unassigned.push(blob);
            continue;
        }
        let index = value - 1;
        if positions[index].is_none() {
            found += 1;
        }
        positions[index] = Some((blob.x, blob.y));
    }

    Ok(Discovery {
        positions,
        unassigned,
        found,
        expected: led_count,
    })
}

/// Turn discovered positions into a layout.
///
/// `screen` maps a point in the photograph to the screen's own coordinates.
/// LEDs sit around the screen rather than on it, so the mapped positions fall
/// outside `0..1`; they are pulled to the nearest edge, which is where an LED
/// should sample from anyway.
pub fn build_layout(
    discovery: &Discovery,
    screen: &Homography,
    display: &str,
    depth: f32,
) -> Layout {
    let depth = depth.clamp(0.02, 0.5);
    let mut leds = Vec::with_capacity(discovery.positions.len());

    for (index, position) in discovery.positions.iter().enumerate() {
        let Some((x, y)) = position else {
            // An LED the camera never saw stays in the chain, dark, so the
            // wire order is not disturbed.
            leds.push(LedSpec {
                index: index as u32,
                display: String::new(),
                rect: Rect::new(0.0, 0.0, 0.05, 0.05),
                weight: 1.0,
                enabled: false,
            });
            continue;
        };
        let (u, v) = screen.map(*x, *y);
        let rect = sample_rect(u, v, depth);
        leds.push(LedSpec::new(index as u32, display, rect));
    }

    let mut layout = Layout {
        name: String::from("Discovered"),
        leds,
        ..Default::default()
    };
    layout.normalise();
    layout
}

/// Where an LED at screen coordinates `(u, v)` should sample from.
///
/// A point outside the screen is pulled to the nearest edge and given a
/// rectangle reaching inwards; a point inside keeps its place.
fn sample_rect(u: f32, v: f32, depth: f32) -> Rect {
    let outside_left = -u;
    let outside_right = u - 1.0;
    let outside_top = -v;
    let outside_bottom = v - 1.0;
    let worst = outside_left
        .max(outside_right)
        .max(outside_top)
        .max(outside_bottom);

    if worst <= 0.0 {
        // Inside the screen: sample around the point itself.
        return Rect::centred(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0), depth, depth).clamped();
    }

    let along_v = v.clamp(0.0, 1.0);
    let along_u = u.clamp(0.0, 1.0);
    let rect = if worst == outside_left {
        Rect::new(0.0, along_v - depth * 0.5, depth, depth)
    } else if worst == outside_right {
        Rect::new(1.0 - depth, along_v - depth * 0.5, depth, depth)
    } else if worst == outside_top {
        Rect::new(along_u - depth * 0.5, 0.0, depth, depth)
    } else {
        Rect::new(along_u - depth * 0.5, 1.0 - depth, depth, depth)
    };
    rect.clamped()
}
