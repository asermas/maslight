//! What the engine reports about itself.
//!
//! The dashboard, the tray tooltip and the local API all read this one struct,
//! so it is the single place where "is it working" is answered.

use maslight_core::{LightMode, Rgb8};
use serde::{Deserialize, Serialize};

/// Health of one output device.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub label: String,
    pub connected: bool,
    pub error: Option<String>,
}

/// A snapshot of the engine.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    /// The worker thread is alive.
    pub running: bool,
    /// The master switch is on.
    pub enabled: bool,
    pub profile: String,
    pub profile_id: String,
    pub mode: LightMode,
    /// Frames delivered in the last second.
    pub fps: f32,
    /// Wall time of the last capture-to-send cycle.
    pub frame_ms: f32,
    /// The screen has been still long enough that capture slowed down.
    pub idle: bool,
    pub led_count: usize,
    /// Colours currently on the strip, for the live preview.
    pub leds: Vec<Rgb8>,
    pub devices: Vec<DeviceStatus>,
    pub capture_backend: String,
    pub display: String,
    pub source_width: u32,
    pub source_height: u32,
    /// Detected letterbox insets: top, bottom, left, right.
    pub insets: [f32; 4],
    /// Total frames sent since start.
    pub frames: u64,
    pub last_error: Option<String>,
}

impl Default for EngineStatus {
    fn default() -> Self {
        Self {
            running: false,
            enabled: false,
            profile: String::new(),
            profile_id: String::new(),
            mode: LightMode::Screen,
            fps: 0.0,
            frame_ms: 0.0,
            idle: false,
            led_count: 0,
            leds: Vec::new(),
            devices: Vec::new(),
            capture_backend: String::from("none"),
            display: String::new(),
            source_width: 0,
            source_height: 0,
            insets: [0.0; 4],
            frames: 0,
            last_error: None,
        }
    }
}

impl EngineStatus {
    /// One-line summary for the tray tooltip.
    pub fn summary(&self) -> String {
        if !self.enabled {
            return String::from("MasLight - off");
        }
        let devices = self.devices.iter().filter(|d| d.connected).count();
        format!(
            "MasLight - {} - {:.0} fps - {} LEDs - {} device(s)",
            self.profile, self.fps, self.led_count, devices
        )
    }
}
