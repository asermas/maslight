//! X11 capture through the MIT-SHM extension.
//!
//! `GetImage` on its own copies the whole screen down the X socket, which at
//! 1920x1080x60 is half a gigabyte a second of pure waste. MIT-SHM hands the
//! server a shared segment instead: the server writes the pixels there and
//! nothing crosses the socket but a small request.
//!
//! This backend covers X11 sessions. Wayland sessions have no root window to
//! grab and go through the PipeWire portal instead.

use std::ptr;
use std::time::Duration;

use maslight_core::{CaptureBackendKind, CaptureSettings, FrameView, PixelFormat};
use x11rb::connection::Connection;
use x11rb::protocol::randr::{self, ConnectionExt as RandrExt};
use x11rb::protocol::shm::{self, ConnectionExt as ShmExt};
use x11rb::protocol::xproto::{ConnectionExt as XExt, ImageFormat, Window};
use x11rb::rust_connection::RustConnection;

use crate::{CaptureBackend, CaptureError, DisplayInfo, FrameStatus};

/// A System V shared memory segment attached to both us and the X server.
struct SharedSegment {
    seg: shm::Seg,
    addr: *mut u8,
    len: usize,
}

impl SharedSegment {
    fn new(conn: &RustConnection, len: usize) -> Result<Self, CaptureError> {
        // SAFETY: plain System V shared memory calls with checked results.
        unsafe {
            let id = libc::shmget(libc::IPC_PRIVATE, len, libc::IPC_CREAT | 0o600);
            if id < 0 {
                return Err(CaptureError::Platform(String::from(
                    "shmget failed, is /dev/shm available?",
                )));
            }
            let addr = libc::shmat(id, ptr::null(), 0);
            if addr == usize::MAX as *mut libc::c_void {
                libc::shmctl(id, libc::IPC_RMID, ptr::null_mut());
                return Err(CaptureError::Platform(String::from("shmat failed")));
            }
            let seg = conn
                .generate_id()
                .map_err(|e| CaptureError::Platform(e.to_string()))?;
            conn.shm_attach(seg, id as u32, false)
                .map_err(|e| CaptureError::Platform(format!("shm_attach: {e}")))?
                .check()
                .map_err(|e| CaptureError::Platform(format!("shm_attach: {e}")))?;
            // Marking the segment for deletion now means it disappears when the
            // last process detaches, even if we crash.
            libc::shmctl(id, libc::IPC_RMID, ptr::null_mut());
            Ok(Self {
                seg,
                addr: addr as *mut u8,
                len,
            })
        }
    }

    fn as_slice(&self) -> &[u8] {
        // SAFETY: the segment is attached for the lifetime of this struct and
        // the server only writes inside `len`.
        unsafe { std::slice::from_raw_parts(self.addr, self.len) }
    }
}

impl Drop for SharedSegment {
    fn drop(&mut self) {
        // SAFETY: detaching a segment we attached.
        unsafe {
            libc::shmdt(self.addr as *const libc::c_void);
        }
    }
}

pub struct X11Backend {
    conn: RustConnection,
    root: Window,
    root_size: (u16, u16),
    has_shm: bool,
    session: Option<Session>,
}

struct Session {
    display_id: String,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    segment: Option<SharedSegment>,
    fallback: Vec<u8>,
}

impl X11Backend {
    pub fn new() -> Result<Self, CaptureError> {
        let (conn, screen_num) = x11rb::connect(None)
            .map_err(|e| CaptureError::Platform(format!("cannot reach the X server: {e}")))?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let root_size = (screen.width_in_pixels, screen.height_in_pixels);

        // MIT-SHM is present on every desktop X server, but a remote display
        // will not have it, and then we fall back to plain GetImage.
        let has_shm = conn
            .shm_query_version()
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some();
        if !has_shm {
            tracing::warn!("MIT-SHM is unavailable, falling back to GetImage");
        }

        Ok(Self {
            conn,
            root,
            root_size,
            has_shm,
            session: None,
        })
    }

    /// Monitors reported by RandR, falling back to the whole root window.
    fn monitors(&self) -> Vec<DisplayInfo> {
        let mut out = Vec::new();
        if let Ok(reply) = self
            .conn
            .randr_get_monitors(self.root, true)
            .and_then(|c| Ok(c.reply()))
        {
            if let Ok(monitors) = reply {
                for (i, m) in monitors.monitors.iter().enumerate() {
                    let name = self
                        .conn
                        .get_atom_name(m.name)
                        .ok()
                        .and_then(|c| c.reply().ok())
                        .map(|r| String::from_utf8_lossy(&r.name).to_string())
                        .unwrap_or_else(|| format!("output-{i}"));
                    out.push(DisplayInfo {
                        id: name.clone(),
                        label: name,
                        width: m.width as u32,
                        height: m.height as u32,
                        x: m.x as i32,
                        y: m.y as i32,
                        primary: m.primary,
                    });
                }
            }
        }
        if out.is_empty() {
            out.push(DisplayInfo {
                id: String::from("root"),
                label: String::from("Screen"),
                width: self.root_size.0 as u32,
                height: self.root_size.1 as u32,
                x: 0,
                y: 0,
                primary: true,
            });
        }
        out
    }
}

impl CaptureBackend for X11Backend {
    fn kind(&self) -> CaptureBackendKind {
        CaptureBackendKind::X11
    }

    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        Ok(self.monitors())
    }

    fn start(
        &mut self,
        display: Option<&str>,
        _settings: &CaptureSettings,
    ) -> Result<(), CaptureError> {
        self.stop();
        let monitors = self.monitors();
        let chosen = match display {
            Some(id) => monitors
                .iter()
                .find(|m| m.id == id)
                .ok_or_else(|| CaptureError::NoSuchDisplay(id.to_string()))?,
            None => monitors
                .iter()
                .find(|m| m.primary)
                .or_else(|| monitors.first())
                .ok_or(CaptureError::NoBackend)?,
        };

        let width = chosen.width.max(1) as u16;
        let height = chosen.height.max(1) as u16;
        let len = width as usize * height as usize * 4;

        let segment = if self.has_shm {
            match SharedSegment::new(&self.conn, len) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!("shared memory unavailable: {e}");
                    self.has_shm = false;
                    None
                }
            }
        } else {
            None
        };

        tracing::info!(
            "X11 capture on {} at {}x{}{}",
            chosen.id,
            width,
            height,
            if segment.is_some() {
                " via MIT-SHM"
            } else {
                ""
            }
        );

        self.session = Some(Session {
            display_id: chosen.id.clone(),
            x: chosen.x as i16,
            y: chosen.y as i16,
            width,
            height,
            segment,
            fallback: Vec::new(),
        });
        Ok(())
    }

    fn next_frame(
        &mut self,
        _timeout: Duration,
        on_frame: &mut dyn FnMut(&FrameView<'_>),
    ) -> Result<FrameStatus, CaptureError> {
        let session = self.session.as_mut().ok_or(CaptureError::NotStarted)?;
        let stride = session.width as usize * 4;

        if let Some(segment) = &session.segment {
            self.conn
                .shm_get_image(
                    self.root,
                    session.x,
                    session.y,
                    session.width,
                    session.height,
                    !0,
                    ImageFormat::Z_PIXMAP.into(),
                    segment.seg,
                    0,
                )
                .map_err(|e| CaptureError::Platform(format!("shm_get_image: {e}")))?
                .reply()
                .map_err(|e| CaptureError::Platform(format!("shm_get_image: {e}")))?;

            let view = FrameView::new(
                segment.as_slice(),
                session.width as u32,
                session.height as u32,
                stride,
                PixelFormat::Bgra8,
            );
            on_frame(&view);
            return Ok(FrameStatus::New);
        }

        // Fallback: pull the image through the socket. Slower, but it keeps
        // remote and unusual servers working.
        let reply = self
            .conn
            .get_image(
                ImageFormat::Z_PIXMAP,
                self.root,
                session.x,
                session.y,
                session.width,
                session.height,
                !0,
            )
            .map_err(|e| CaptureError::Platform(format!("get_image: {e}")))?
            .reply()
            .map_err(|e| CaptureError::Platform(format!("get_image: {e}")))?;

        session.fallback = reply.data;
        let view = FrameView::new(
            &session.fallback,
            session.width as u32,
            session.height as u32,
            stride,
            PixelFormat::Bgra8,
        );
        on_frame(&view);
        Ok(FrameStatus::New)
    }

    fn stop(&mut self) {
        if let Some(session) = self.session.take() {
            if let Some(segment) = &session.segment {
                let _ = self.conn.shm_detach(segment.seg);
            }
        }
    }

    fn current_size(&self) -> Option<(u32, u32)> {
        self.session
            .as_ref()
            .map(|s| (s.width as u32, s.height as u32))
    }
}

impl X11Backend {
    /// Display this session is bound to, if any.
    pub fn current_display(&self) -> Option<&str> {
        self.session.as_ref().map(|s| s.display_id.as_str())
    }
}

// The connection is only ever used from the engine worker thread.
unsafe impl Send for X11Backend {}

// Keep the randr import meaningful even when only monitors are queried.
const _: Option<randr::MonitorInfo> = None;
