//! # maslight-capture
//!
//! Screen capture, one backend per platform behind a single trait.
//!
//! A backend hands the engine a borrowed [`FrameView`] rather than an owned
//! buffer: the pixels usually live in a mapped GPU staging surface that has to
//! be released before the next frame, and a callback makes that lifetime
//! impossible to get wrong.
//!
//! Backends downscale on the GPU wherever the platform allows it, so what
//! crosses the bus is a small image rather than a full desktop.

use std::time::Duration;

use maslight_core::{CaptureBackendKind, CaptureSettings, FrameView};

pub mod test_source;

#[cfg(target_os = "windows")]
pub mod dxgi;

#[cfg(target_os = "linux")]
pub mod x11;

#[cfg(all(target_os = "linux", feature = "portal"))]
pub mod portal;

#[cfg(all(target_os = "linux", feature = "wayland"))]
pub mod pipewire;

/// One capturable display.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    /// Stable platform identifier used in layouts.
    pub id: String,
    /// Human label, e.g. the monitor model or `Display 1`.
    pub label: String,
    pub width: u32,
    pub height: u32,
    /// Position on the virtual desktop.
    pub x: i32,
    pub y: i32,
    pub primary: bool,
}

/// What happened when the engine asked for a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStatus {
    /// A new frame was delivered to the callback.
    New,
    /// Nothing on screen changed, so the previous colours still stand.
    Unchanged,
    /// Nothing arrived inside the timeout.
    Timeout,
    /// The session ended and the backend needs restarting, usually because
    /// the resolution changed, the session locked, or a fullscreen game took
    /// the display.
    Lost,
}

/// Failures a capture backend can report.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("no capture backend is available on this system")]
    NoBackend,
    #[error("display {0} was not found")]
    NoSuchDisplay(String),
    #[error("capture has not been started")]
    NotStarted,
    #[error("permission to capture the screen was refused")]
    PermissionDenied,
    #[error("capture failed: {0}")]
    Platform(String),
}

/// A source of frames.
pub trait CaptureBackend: Send {
    fn kind(&self) -> CaptureBackendKind;

    /// Displays this backend can capture.
    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError>;

    /// Begin capturing. `display` is an id from [`CaptureBackend::displays`];
    /// `None` means the primary display.
    fn start(
        &mut self,
        display: Option<&str>,
        settings: &CaptureSettings,
    ) -> Result<(), CaptureError>;

    /// Wait for the next frame and hand it to `on_frame`.
    ///
    /// The view is only valid for the duration of the call.
    fn next_frame(
        &mut self,
        timeout: Duration,
        on_frame: &mut dyn FnMut(&FrameView<'_>),
    ) -> Result<FrameStatus, CaptureError>;

    /// Release the capture session. Called when the engine idles so a game can
    /// take exclusive fullscreen without fighting us for the display.
    fn stop(&mut self);

    /// Geometry of the surface currently being captured, if any.
    fn current_size(&self) -> Option<(u32, u32)>;
}

/// The backend that suits this platform.
pub fn default_kind() -> CaptureBackendKind {
    #[cfg(target_os = "windows")]
    {
        CaptureBackendKind::Dxgi
    }
    #[cfg(target_os = "linux")]
    {
        // Wayland sessions have no usable X11 root window to grab, so the
        // portal is the only correct choice there.
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            CaptureBackendKind::PipeWire
        } else {
            CaptureBackendKind::X11
        }
    }
    #[cfg(target_os = "macos")]
    {
        CaptureBackendKind::ScreenCaptureKit
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        CaptureBackendKind::Test
    }
}

/// Build a backend, resolving [`CaptureBackendKind::Auto`] for this platform.
pub fn create_backend(kind: CaptureBackendKind) -> Result<Box<dyn CaptureBackend>, CaptureError> {
    let kind = if kind == CaptureBackendKind::Auto {
        default_kind()
    } else {
        kind
    };

    match kind {
        CaptureBackendKind::Test => Ok(Box::new(test_source::TestBackend::default())),

        #[cfg(target_os = "windows")]
        CaptureBackendKind::Dxgi => Ok(Box::new(dxgi::DxgiBackend::new()?)),

        #[cfg(target_os = "linux")]
        CaptureBackendKind::X11 => Ok(Box::new(x11::X11Backend::new()?)),

        #[cfg(all(target_os = "linux", feature = "wayland"))]
        CaptureBackendKind::PipeWire => Ok(Box::new(pipewire::PipeWireBackend::new()?)),

        _ => Err(CaptureError::NoBackend),
    }
}

/// Backends that can actually run here, for the settings screen.
pub fn available_kinds() -> Vec<CaptureBackendKind> {
    let mut kinds = vec![CaptureBackendKind::Auto];
    #[cfg(target_os = "windows")]
    kinds.push(CaptureBackendKind::Dxgi);
    #[cfg(target_os = "linux")]
    {
        kinds.push(CaptureBackendKind::X11);
        kinds.push(CaptureBackendKind::PipeWire);
    }
    #[cfg(target_os = "macos")]
    kinds.push(CaptureBackendKind::ScreenCaptureKit);
    kinds.push(CaptureBackendKind::Test);
    kinds
}
