//! A synthetic capture source.
//!
//! Used by the tests, by continuous integration where there is no display at
//! all, and by the layout editor to preview a mapping before any hardware is
//! connected. It produces a deterministic moving pattern, so a test can assert
//! on exact colours.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use maslight_core::{CaptureBackendKind, CaptureSettings, FrameView, PixelFormat};

use crate::{CaptureBackend, CaptureError, DisplayInfo, FrameStatus};

/// How many more calls to `start` should fail before one is allowed to work.
///
/// A capture backend that refuses to start is ordinary: Desktop Duplication is
/// exclusive per output, so a previous instance still shutting down, a game in
/// exclusive fullscreen or a driver reset all make it fail for a few seconds.
/// The engine has to come back and try again rather than leaving the lights
/// dark forever, and that behaviour cannot be tested without a backend that
/// fails on demand.
///
/// Process-wide because the engine owns its backend on its own thread and
/// there is no seam to hand one in, so anything using this has to make sure no
/// other test is starting a capture at the same time. The engine's end to end
/// tests take a lock for exactly that reason.
static FAILING_STARTS: AtomicU32 = AtomicU32::new(0);

/// Make the next `n` attempts to start a test backend fail.
pub fn fail_next_starts(n: u32) {
    FAILING_STARTS.store(n, Ordering::SeqCst);
}

/// How many failures are still owed, for a test to assert they were used up.
pub fn remaining_failing_starts() -> u32 {
    FAILING_STARTS.load(Ordering::SeqCst)
}

/// What the synthetic display shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TestPattern {
    /// Four solid quadrants: red, green, blue, white.
    #[default]
    Quadrants,
    /// A hue sweep that scrolls one column per frame.
    ScrollingHue,
    /// A single flat colour, useful for latency work.
    Solid,
    /// Never changes, so adaptive frame skipping can be exercised.
    Static,
}

pub struct TestBackend {
    width: u32,
    height: u32,
    pattern: TestPattern,
    frame: u64,
    buffer: Vec<u8>,
    started: bool,
    solid: [u8; 3],
}

impl Default for TestBackend {
    fn default() -> Self {
        Self::new(640, 360, TestPattern::Quadrants)
    }
}

impl TestBackend {
    pub fn new(width: u32, height: u32, pattern: TestPattern) -> Self {
        Self {
            width,
            height,
            pattern,
            frame: 0,
            buffer: vec![0; (width * height * 4) as usize],
            started: false,
            solid: [255, 255, 255],
        }
    }

    /// Colour used by [`TestPattern::Solid`].
    pub fn set_solid(&mut self, rgb: [u8; 3]) {
        self.solid = rgb;
    }

    fn render(&mut self) {
        let w = self.width;
        let h = self.height;
        let frame = self.frame;
        let pattern = self.pattern;
        let solid = self.solid;

        for y in 0..h {
            for x in 0..w {
                let p = ((y * w + x) * 4) as usize;
                let [r, g, b] = match pattern {
                    TestPattern::Quadrants => {
                        let left = x < w / 2;
                        let top = y < h / 2;
                        match (left, top) {
                            (true, true) => [255, 0, 0],
                            (false, true) => [0, 255, 0],
                            (true, false) => [0, 0, 255],
                            (false, false) => [255, 255, 255],
                        }
                    }
                    TestPattern::ScrollingHue => {
                        let hue = ((x as u64 + frame) % w as u64) as f32 / w as f32;
                        hue_to_rgb8(hue)
                    }
                    TestPattern::Solid => solid,
                    TestPattern::Static => [32, 64, 96],
                };
                // BGRA, matching what Desktop Duplication hands back.
                self.buffer[p] = b;
                self.buffer[p + 1] = g;
                self.buffer[p + 2] = r;
                self.buffer[p + 3] = 255;
            }
        }
    }
}

impl CaptureBackend for TestBackend {
    fn kind(&self) -> CaptureBackendKind {
        CaptureBackendKind::Test
    }

    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        Ok(vec![DisplayInfo {
            id: String::from("test"),
            label: String::from("Test pattern"),
            width: self.width,
            height: self.height,
            x: 0,
            y: 0,
            primary: true,
        }])
    }

    fn start(
        &mut self,
        _display: Option<&str>,
        _settings: &CaptureSettings,
    ) -> Result<(), CaptureError> {
        // Owed a failure? Spend one. See FAILING_STARTS.
        if FAILING_STARTS
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            return Err(CaptureError::Platform(String::from(
                "the test backend was asked to fail this start",
            )));
        }
        self.started = true;
        self.frame = 0;
        Ok(())
    }

    fn next_frame(
        &mut self,
        _timeout: Duration,
        on_frame: &mut dyn FnMut(&FrameView<'_>),
    ) -> Result<FrameStatus, CaptureError> {
        if !self.started {
            return Err(CaptureError::NotStarted);
        }
        if self.pattern == TestPattern::Static && self.frame > 0 {
            self.frame += 1;
            return Ok(FrameStatus::Unchanged);
        }
        self.render();
        self.frame += 1;
        let view = FrameView::new(
            &self.buffer,
            self.width,
            self.height,
            (self.width * 4) as usize,
            PixelFormat::Bgra8,
        );
        on_frame(&view);
        Ok(FrameStatus::New)
    }

    fn stop(&mut self) {
        self.started = false;
    }

    fn current_size(&self) -> Option<(u32, u32)> {
        self.started.then_some((self.width, self.height))
    }
}

/// Fully saturated hue to 8-bit RGB.
fn hue_to_rgb8(h: f32) -> [u8; 3] {
    let h = (h.fract() + 1.0).fract() * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let q = 1.0 - f;
    let (r, g, b) = match i {
        0 => (1.0, f, 0.0),
        1 => (q, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, q, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, q),
    };
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}
