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
    /// The transport is open and frames are being written to it.
    ///
    /// For UDP this is not proof that anything is listening: a datagram sent
    /// into the void succeeds. Reachability is reported separately.
    pub connected: bool,
    /// Whether the controller answered when we last asked it about itself.
    /// `None` means the question does not apply to this transport.
    pub reachable: Option<bool>,
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
    /// Audio capture is running and contributing to the frame.
    pub audio_active: bool,
    /// Overall loudness the audio engine last measured, 0..=1.
    pub audio_energy: f32,
    /// The active profile was last chosen by a rule rather than by hand.
    pub rule_switched: bool,
    /// What the rule sampler last saw, so the rules screen can show why a
    /// rule is or is not firing.
    pub rule_fullscreen: bool,
    pub rule_on_battery: bool,
    /// Local time as minutes past midnight, from the same clock the rules use.
    pub rule_minutes: u16,
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
            audio_active: false,
            audio_energy: 0.0,
            rule_switched: false,
            rule_fullscreen: false,
            rule_on_battery: false,
            rule_minutes: 0,
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
