//! # maslight-core
//!
//! The parts of MasLight that have nothing to do with any operating system:
//! the colour pipeline, the LED layout model, zone reduction and the profile
//! format. Everything here is pure Rust with no platform dependencies, which
//! is what makes it testable and what keeps the platform crates small.
//!
//! ```
//! use maslight_core::{ColorPipeline, ColorSettings, Rgb};
//!
//! let mut pipeline = ColorPipeline::new(ColorSettings {
//!     smoothing_ms: 0.0,
//!     dithering: false,
//!     ..Default::default()
//! });
//! let frame = pipeline.process(&[Rgb::WHITE], 1.0 / 60.0);
//! assert_eq!(frame.rgb[0].r, 255);
//! ```

pub mod audio;
pub mod color;
pub mod layout;
pub mod profile;
pub mod reduce;
pub mod types;

pub use audio::{AudioEffect, AudioSettings, Palette};
pub use color::{
    linear_to_srgb, srgb_to_linear, ColorPipeline, ColorSettings, LuminancePolicy, RgbwMode,
};
pub use layout::{
    ChainConfig, Corner, Direction, DisplayRegion, EdgeCounts, Layout, LedSpec, Rect, WizardParams,
};
pub use profile::{
    AppConfig, AutoRule, CaptureBackendKind, CaptureSettings, ConfigError, DeviceConfig, LightMode,
    Profile, SerialProtocol, UiPrefs, WledProtocol,
};
pub use reduce::{detect_black_bars, f16_to_f32, FrameView, Insets, PixelFormat, Reducer};
pub use types::{ColorOrder, LedFrame, Rgb, Rgb8};

/// Version of the crate, surfaced in the UI and in the API handshake.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
