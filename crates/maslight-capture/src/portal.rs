//! The xdg-desktop-portal ScreenCast handshake.
//!
//! On Wayland there is no root window to grab. Everything goes through the
//! portal: the compositor asks the person which screen to share, and hands
//! back a PipeWire remote plus a node id.
//!
//! The part that matters for daily use is the **restore token**. Without it
//! the consent dialog appears every single time the app starts, which is what
//! makes most Wayland capture tools unpleasant. MasLight asks for a token,
//! stores it next to the configuration, and hands it back on the next run, so
//! the dialog is answered once.

use std::os::fd::OwnedFd;
use std::path::PathBuf;

use ashpd::desktop::screencast::{
    CursorMode, OpenPipeWireRemoteOptions, Screencast, SelectSourcesOptions, SourceType,
    StartCastOptions,
};
use ashpd::desktop::CreateSessionOptions;
use ashpd::desktop::PersistMode;

use crate::CaptureError;

/// What the portal gives us once the person has agreed.
pub struct PortalStream {
    /// File descriptor of the PipeWire remote to connect to.
    pub fd: OwnedFd,
    /// Node id of the stream inside that remote.
    pub node_id: u32,
    /// Size the compositor reported, when it reported one.
    pub size: Option<(i32, i32)>,
    /// Human name of the captured output, when the portal supplied one.
    pub label: Option<String>,
}

/// Where the restore token is kept.
///
/// Deliberately a separate file from the configuration: it is a machine local
/// credential, it should not travel with a profile that someone exports, and a
/// user who wants the consent dialog back only has to delete it.
pub fn token_path() -> PathBuf {
    maslight_core::profile::config_dir().join("portal-token")
}

fn read_token() -> Option<String> {
    std::fs::read_to_string(token_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn write_token(token: &str) {
    let path = token_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, token) {
        tracing::warn!("could not store the portal token: {e}");
    }
}

/// Forget the stored consent, so the next start asks again.
pub fn forget_token() {
    let _ = std::fs::remove_file(token_path());
}

/// True when this session is Wayland and the portal is the right route.
pub fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false)
}

/// Run the whole handshake and return a stream to read from.
///
/// This blocks, because the engine worker is a plain thread and the portal
/// call is a one-off at start-up. The consent dialog means it can take as long
/// as the person takes to answer it.
pub fn open_screencast() -> Result<PortalStream, CaptureError> {
    pollster::block_on(open_screencast_async())
}

async fn open_screencast_async() -> Result<PortalStream, CaptureError> {
    let proxy = Screencast::new()
        .await
        .map_err(|e| CaptureError::Platform(format!("no screencast portal: {e}")))?;

    let session = proxy
        .create_session(CreateSessionOptions::default())
        .await
        .map_err(|e| CaptureError::Platform(format!("portal session: {e}")))?;

    let restore = read_token();
    let options = SelectSourcesOptions::default()
        // The pointer is not part of the picture we are averaging, and
        // including it would make the strip flicker as the mouse moves.
        .set_cursor_mode(CursorMode::Hidden)
        .set_sources(enumflags2::BitFlags::from(SourceType::Monitor))
        .set_multiple(false)
        .set_restore_token(restore.as_deref())
        // The token stays valid until the person revokes it in the system
        // settings, which is the behaviour that removes the dialog.
        .set_persist_mode(PersistMode::ExplicitlyRevoked);

    proxy
        .select_sources(&session, options)
        .await
        .map_err(|e| CaptureError::Platform(format!("portal select_sources: {e}")))?;

    let response = proxy
        .start(&session, None, StartCastOptions::default())
        .await
        .map_err(|e| CaptureError::Platform(format!("portal start: {e}")))?
        .response()
        .map_err(|_| CaptureError::PermissionDenied)?;

    if let Some(token) = response.restore_token() {
        write_token(token);
    }

    let stream = response
        .streams()
        .first()
        .ok_or_else(|| CaptureError::Platform(String::from("the portal returned no stream")))?
        .clone();

    let fd = proxy
        .open_pipe_wire_remote(&session, OpenPipeWireRemoteOptions::default())
        .await
        .map_err(|e| CaptureError::Platform(format!("portal pipewire remote: {e}")))?;

    Ok(PortalStream {
        fd,
        node_id: stream.pipe_wire_node_id(),
        size: stream.size(),
        label: stream.id().map(str::to_string),
    })
}
