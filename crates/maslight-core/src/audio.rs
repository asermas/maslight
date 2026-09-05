//! Audio mode settings.
//!
//! The types live here rather than in `maslight-audio` so a profile can be
//! serialised without pulling in a sound library. The capture and analysis
//! code reads them; nothing here knows what a device is.

use serde::{Deserialize, Serialize};

/// How the spectrum is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioEffect {
    /// Bass at both ends, treble in the middle, mirrored. Reads well on a
    /// strip that runs around a screen because the low end lands in the
    /// corners nearest the desk.
    #[default]
    Spectrum,
    /// The whole strip pulses with overall loudness in one hue.
    Energy,
    /// A wave travelling out from the middle, driven by the beat.
    Wave,
    /// A hue that scrolls continuously, brightness following the music.
    Scroll,
}

/// Colour choices shared by the effects.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Palette {
    /// Hue at the bass end, 0..1.
    pub from_hue: f32,
    /// Hue at the treble end, 0..1.
    pub to_hue: f32,
    pub saturation: f32,
}

impl Default for Palette {
    fn default() -> Self {
        // Deep blue through magenta to warm amber: the low end sits cool, the
        // top end sits warm, which is how people expect a spectrum to read.
        Self {
            from_hue: 0.62,
            to_hue: 0.08,
            saturation: 1.0,
        }
    }
}

/// Everything a profile stores about the audio mode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AudioSettings {
    /// Capture device name. `None` follows the default output.
    pub device: Option<String>,
    pub bands: u32,
    /// Extra gain applied after normalisation.
    pub gain: f32,
    /// Level in decibels that maps to nothing.
    pub floor_db: f32,
    /// Level in decibels that maps to full.
    pub ceiling_db: f32,
    /// How fast a band rises, in milliseconds.
    pub attack_ms: f32,
    /// How fast a band falls, in milliseconds.
    pub release_ms: f32,
    /// Standard deviations above the running mean that count as a beat.
    pub beat_sensitivity: f32,
    /// How much a beat brightens the frame, 0..=1.
    pub beat_boost: f32,
    /// Hue drift per second for the scrolling effect.
    pub scroll_speed: f32,
    pub effect: AudioEffect,
    pub palette: Palette,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            device: None,
            bands: 16,
            gain: 1.0,
            // Typical music sits around -20 dBFS, so this range puts normal
            // listening in the middle of the strip rather than at either end.
            floor_db: -62.0,
            ceiling_db: -12.0,
            attack_ms: 18.0,
            release_ms: 220.0,
            beat_sensitivity: 1.4,
            beat_boost: 0.35,
            scroll_speed: 0.08,
            effect: AudioEffect::Spectrum,
            palette: Palette::default(),
        }
    }
}

impl AudioSettings {
    pub fn sanitise(&mut self) {
        self.bands = self.bands.clamp(2, 64);
        self.gain = finite(self.gain, 1.0).clamp(0.1, 8.0);
        self.floor_db = finite(self.floor_db, -62.0).clamp(-100.0, -20.0);
        self.ceiling_db = finite(self.ceiling_db, -12.0).clamp(self.floor_db + 6.0, 0.0);
        self.attack_ms = finite(self.attack_ms, 18.0).clamp(0.0, 2000.0);
        self.release_ms = finite(self.release_ms, 220.0).clamp(0.0, 5000.0);
        self.beat_sensitivity = finite(self.beat_sensitivity, 1.4).clamp(0.2, 6.0);
        self.beat_boost = finite(self.beat_boost, 0.35).clamp(0.0, 1.0);
        self.scroll_speed = finite(self.scroll_speed, 0.08).clamp(-2.0, 2.0);
        self.palette.saturation = finite(self.palette.saturation, 1.0).clamp(0.0, 1.0);
        self.palette.from_hue = finite(self.palette.from_hue, 0.62).rem_euclid(1.0);
        self.palette.to_hue = finite(self.palette.to_hue, 0.08).rem_euclid(1.0);
    }
}

fn finite(v: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}
