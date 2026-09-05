//! Profiles, device descriptions and the on-disk configuration.
//!
//! A [`Profile`] is a complete description of one look: what to capture, how
//! to process it, where to send it. [`AppConfig`] holds every profile plus the
//! application preferences and knows how to load and save itself.
//!
//! Device *descriptions* live here rather than in `maslight-output` so that a
//! profile can be serialised without pulling in any transport code.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::audio::AudioSettings;
use crate::color::ColorSettings;
use crate::layout::Layout;

/// Realtime protocol spoken to a WLED controller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WledProtocol {
    /// Index + RGB triplets. Compact when only a few LEDs change.
    Warls,
    /// Straight RGB stream from LED 0. Caps out at 490 LEDs.
    Drgb,
    /// DRGB with a 16-bit start index, so it can be split across packets.
    #[default]
    Dnrgb,
    /// DRGB plus a white channel.
    Drgbw,
    /// DNRGB plus a white channel.
    Dnrgbw,
}

/// Framing used over a serial link.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SerialProtocol {
    /// The classic `Ada` header used by Adalight and Prismatik.
    #[default]
    Adalight,
    /// TPM2 framing.
    Tpm2,
}

/// One output device.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum DeviceConfig {
    /// A WLED controller reached over UDP realtime.
    Wled {
        host: String,
        #[serde(default = "wled_port")]
        port: u16,
        #[serde(default)]
        protocol: WledProtocol,
        /// Seconds WLED waits after the last packet before it returns to its
        /// own effects. 1..255, or 255 to hold until told otherwise.
        #[serde(default = "wled_timeout")]
        timeout_s: u8,
    },
    /// Distributed Display Protocol, supported by WLED 0.11+ and many others.
    Ddp {
        host: String,
        #[serde(default = "ddp_port")]
        port: u16,
        #[serde(default)]
        start_offset: u32,
    },
    /// E1.31 / sACN. 170 LEDs per universe.
    E131 {
        /// Unicast target. `None` uses the multicast group for the universe.
        #[serde(default)]
        host: Option<String>,
        #[serde(default = "one_u16")]
        universe: u16,
        #[serde(default)]
        priority: u8,
    },
    /// Art-Net, 170 LEDs per universe.
    ArtNet {
        host: String,
        #[serde(default = "artnet_port")]
        port: u16,
        #[serde(default)]
        universe: u16,
        #[serde(default)]
        net: u8,
        #[serde(default)]
        subnet: u8,
    },
    /// A directly attached Arduino or ESP running Adalight or TPM2.
    Serial {
        port: String,
        #[serde(default = "serial_baud")]
        baud: u32,
        #[serde(default)]
        protocol: SerialProtocol,
    },
    /// An OpenRGB server, which in turn drives motherboard headers, RAM,
    /// keyboards and anything else it knows about.
    OpenRgb {
        host: String,
        #[serde(default = "openrgb_port")]
        port: u16,
        /// Which device on that server. OpenRGB numbers them from zero.
        #[serde(default)]
        device: u32,
    },
    /// An MQTT broker, for home automation. Publishes state, not frames.
    Mqtt {
        host: String,
        #[serde(default = "mqtt_port")]
        port: u16,
        #[serde(default = "mqtt_topic")]
        topic: String,
    },
    /// Discards frames. Used by tests, the preview and the calibration
    /// wizard before a device is chosen.
    Null,
}

fn wled_port() -> u16 {
    21324
}
fn wled_timeout() -> u8 {
    2
}
fn ddp_port() -> u16 {
    4048
}
fn artnet_port() -> u16 {
    6454
}
fn one_u16() -> u16 {
    1
}
fn serial_baud() -> u32 {
    115_200
}
fn openrgb_port() -> u16 {
    6742
}
fn mqtt_port() -> u16 {
    1883
}
fn mqtt_topic() -> String {
    String::from("maslight/state")
}

impl DeviceConfig {
    /// Short human label for the UI and logs.
    pub fn label(&self) -> String {
        match self {
            DeviceConfig::Wled { host, protocol, .. } => {
                format!("WLED {host} ({protocol:?})")
            }
            DeviceConfig::Ddp { host, .. } => format!("DDP {host}"),
            DeviceConfig::E131 { host, universe, .. } => match host {
                Some(h) => format!("sACN {h} u{universe}"),
                None => format!("sACN multicast u{universe}"),
            },
            DeviceConfig::ArtNet { host, universe, .. } => format!("Art-Net {host} u{universe}"),
            DeviceConfig::Serial { port, .. } => format!("Serial {port}"),
            DeviceConfig::OpenRgb { host, device, .. } => {
                format!("OpenRGB {host} device {device}")
            }
            DeviceConfig::Mqtt { host, topic, .. } => format!("MQTT {host} {topic}"),
            DeviceConfig::Null => String::from("Disconnected"),
        }
    }
}

/// Which capture backend to use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureBackendKind {
    /// Pick the best backend for the running system.
    #[default]
    Auto,
    /// Windows Desktop Duplication.
    Dxgi,
    /// X11 with the MIT-SHM extension.
    X11,
    /// PipeWire through the xdg-desktop-portal screencast interface.
    PipeWire,
    /// macOS ScreenCaptureKit.
    ScreenCaptureKit,
    /// Deterministic synthetic frames, for tests and demos.
    Test,
}

/// Capture tuning.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CaptureSettings {
    pub backend: CaptureBackendKind,
    /// Upper bound on capture rate. The engine never runs faster than this.
    pub target_fps: u32,
    /// Drop to `idle_fps` when the screen stops changing.
    pub adaptive: bool,
    pub idle_fps: u32,
    /// Per-channel change, 0..1, below which a frame counts as unchanged.
    pub idle_threshold: f32,
    /// Treat the source as HDR and tone map it.
    pub hdr: bool,
    /// Detect letterbox bars and pull the sample rectangles inwards.
    pub detect_black_bars: bool,
    /// Samples taken across the long side of each LED rectangle. The reducer
    /// takes at most `sample_grid` squared taps per LED.
    pub sample_grid: u32,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            backend: CaptureBackendKind::Auto,
            target_fps: 60,
            adaptive: true,
            idle_fps: 10,
            idle_threshold: 0.004,
            hdr: false,
            detect_black_bars: true,
            sample_grid: 16,
        }
    }
}

impl CaptureSettings {
    pub fn sanitise(&mut self) {
        self.target_fps = self.target_fps.clamp(1, 240);
        self.idle_fps = self.idle_fps.clamp(1, self.target_fps);
        if !self.idle_threshold.is_finite() {
            self.idle_threshold = 0.004;
        }
        self.idle_threshold = self.idle_threshold.clamp(0.0, 0.5);
        self.sample_grid = self.sample_grid.clamp(2, 64);
    }
}

/// What drives the LEDs in a profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LightMode {
    /// Screen capture ambilight.
    #[default]
    Screen,
    /// Audio reactive effects.
    Audio,
    /// A static colour or a scripted effect.
    Effect,
    /// Everything off, but the device connection is kept warm.
    Off,
}

/// One complete look.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub mode: LightMode,
    pub layout: Layout,
    pub color: ColorSettings,
    pub capture: CaptureSettings,
    pub audio: AudioSettings,
    /// Colour shown in [`LightMode::Effect`] when no script is set.
    pub effect_color: crate::types::Rgb8,
    /// Rhai source for [`LightMode::Effect`]. Empty means the static colour.
    pub script: String,
    pub devices: Vec<DeviceConfig>,
    /// Extra delay in milliseconds applied before sending, so the strip and
    /// the panel change at the same instant. Measured by the wizard.
    pub latency_ms: u32,
    /// Blend between screen (0.0) and audio (1.0) when both are running.
    pub audio_blend: f32,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: String::from("default"),
            name: String::from("Default"),
            mode: LightMode::Screen,
            layout: Layout::default(),
            color: ColorSettings::default(),
            capture: CaptureSettings::default(),
            audio: AudioSettings::default(),
            effect_color: crate::types::Rgb8::new(255, 170, 90),
            script: String::new(),
            devices: Vec::new(),
            latency_ms: 0,
            audio_blend: 0.0,
        }
    }
}

impl Profile {
    pub fn sanitise(&mut self) {
        self.color.sanitise();
        self.capture.sanitise();
        self.audio.sanitise();
        self.layout.normalise();
        self.latency_ms = self.latency_ms.min(2000);
        if !self.audio_blend.is_finite() {
            self.audio_blend = 0.0;
        }
        self.audio_blend = self.audio_blend.clamp(0.0, 1.0);
        if self.id.trim().is_empty() {
            self.id = String::from("default");
        }
    }
}

/// A condition that can switch profiles on its own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "when",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum AutoRule {
    /// A named process is running.
    ProcessRunning { process: String, profile: String },
    /// Some window is fullscreen on the captured display.
    Fullscreen { profile: String },
    /// Local time is inside `[from, to)`, given as minutes past midnight.
    TimeRange {
        from_minutes: u16,
        to_minutes: u16,
        profile: String,
    },
    /// The machine is running on battery.
    OnBattery { profile: String },
}

/// User interface preferences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiPrefs {
    /// `tr`, `en`, or `system`.
    pub language: String,
    /// `dark`, `light`, or `system`.
    pub theme: String,
    pub start_minimised: bool,
    pub launch_at_login: bool,
    /// Off by default. Nothing leaves the machine unless this is turned on.
    pub check_for_updates: bool,
    /// Serve the local REST + WebSocket API.
    pub enable_api: bool,
    pub api_port: u16,
    /// Bearer token the API requires. Generated the first time the API is
    /// turned on; regenerating it revokes every script that had the old one.
    pub api_token: String,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            language: String::from("system"),
            theme: String::from("dark"),
            start_minimised: false,
            launch_at_login: false,
            check_for_updates: false,
            enable_api: false,
            api_port: 4599,
            api_token: String::new(),
        }
    }
}

/// Everything MasLight persists.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppConfig {
    pub version: u32,
    pub active_profile: String,
    pub profiles: Vec<Profile>,
    pub rules: Vec<AutoRule>,
    pub ui: UiPrefs,
    /// Master switch. When false the engine idles and the strip is dark.
    pub enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        let profile = Profile::default();
        Self {
            version: 1,
            active_profile: profile.id.clone(),
            profiles: vec![profile],
            rules: Vec::new(),
            ui: UiPrefs::default(),
            enabled: true,
        }
    }
}

impl AppConfig {
    pub fn sanitise(&mut self) {
        if self.profiles.is_empty() {
            self.profiles.push(Profile::default());
        }
        for p in &mut self.profiles {
            p.sanitise();
        }
        if !self.profiles.iter().any(|p| p.id == self.active_profile) {
            self.active_profile = self.profiles[0].id.clone();
        }
    }

    pub fn active(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|p| p.id == self.active_profile)
            .unwrap_or(&self.profiles[0])
    }

    pub fn active_mut(&mut self) -> &mut Profile {
        let id = self.active_profile.clone();
        let idx = self.profiles.iter().position(|p| p.id == id).unwrap_or(0);
        &mut self.profiles[idx]
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// Insert or replace a profile by id.
    pub fn upsert(&mut self, profile: Profile) {
        match self.profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(slot) => *slot = profile,
            None => self.profiles.push(profile),
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        if self.profiles.len() <= 1 {
            return false;
        }
        let before = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        if self.active_profile == id {
            self.active_profile = self.profiles[0].id.clone();
        }
        self.profiles.len() != before
    }

    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        let mut cfg: AppConfig = serde_json::from_str(&text)?;
        cfg.sanitise();
        Ok(cfg)
    }

    /// Write atomically: a crash mid-save must never leave a truncated config.
    pub fn save_to(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Load from the default location, falling back to defaults when the file
    /// does not exist yet.
    pub fn load_or_default() -> Self {
        let path = config_path();
        match AppConfig::load_from(&path) {
            Ok(cfg) => cfg,
            Err(_) => {
                let mut cfg = AppConfig::default();
                cfg.sanitise();
                cfg
            }
        }
    }
}

/// Errors from reading or writing the configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration is not valid json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Directory MasLight keeps its configuration in.
///
/// * Windows: `%APPDATA%\MasLight`
/// * macOS: `~/Library/Application Support/MasLight`
/// * Linux: `$XDG_CONFIG_HOME/maslight`, else `~/.config/maslight`
pub fn config_dir() -> PathBuf {
    if cfg!(windows) {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("MasLight");
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("MasLight");
        }
    } else if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("maslight");
        }
    }
    match std::env::var("HOME") {
        Ok(home) => PathBuf::from(home).join(".config").join("maslight"),
        Err(_) => PathBuf::from(".maslight"),
    }
}

/// Full path of the configuration file.
pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}
