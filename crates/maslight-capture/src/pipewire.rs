//! Wayland capture: xdg-desktop-portal ScreenCast plus a PipeWire stream.
//!
//! The portal half lives in [`crate::portal`] and produces a file descriptor
//! and a node id. This module connects to that remote, negotiates a plain
//! packed RGB format, and copies each frame into a buffer the engine can read.
//!
//! PipeWire drives its own main loop, so the loop runs on a dedicated thread
//! and hands frames over through a mutex. The engine then samples only the
//! pixels its LEDs need, which is why one copy per frame is affordable.
//!
//! **Status:** written against pipewire 0.10 and ashpd 0.13 but not yet
//! compiled on a Linux machine, which is why it sits behind the `wayland`
//! feature. The X11 backend is the tested Linux path today.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use maslight_core::{CaptureBackendKind, CaptureSettings, FrameView, PixelFormat};
use pipewire as pw;
use pw::spa;
use spa::param::format::{MediaSubtype, MediaType};
use spa::param::format_utils;
use spa::param::video::{VideoFormat, VideoInfoRaw};
use spa::pod::Pod;
use spa::utils::{Direction, Fraction, Rectangle};

use crate::portal::{self, PortalStream};
use crate::{CaptureBackend, CaptureError, DisplayInfo, FrameStatus};

/// What the PipeWire thread publishes for the engine to read.
struct FrameSlot {
    data: Vec<u8>,
    width: u32,
    height: u32,
    stride: usize,
    format: PixelFormat,
}

struct Shared {
    slot: Mutex<FrameSlot>,
    generation: AtomicU64,
    error: Mutex<Option<String>>,
    running: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            slot: Mutex::new(FrameSlot {
                data: Vec::new(),
                width: 0,
                height: 0,
                stride: 0,
                format: PixelFormat::Bgra8,
            }),
            generation: AtomicU64::new(0),
            error: Mutex::new(None),
            running: AtomicBool::new(false),
        }
    }

    fn fail(&self, message: String) {
        tracing::warn!("pipewire: {message}");
        *self.error.lock().unwrap() = Some(message);
        self.running.store(false, Ordering::SeqCst);
    }
}

pub struct PipeWireBackend {
    shared: Arc<Shared>,
    quit: Option<pw::channel::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    seen: u64,
    label: String,
    size: Option<(u32, u32)>,
}

impl PipeWireBackend {
    pub fn new() -> Result<Self, CaptureError> {
        Ok(Self {
            shared: Arc::new(Shared::new()),
            quit: None,
            thread: None,
            seen: 0,
            label: String::from("portal"),
            size: None,
        })
    }
}

impl CaptureBackend for PipeWireBackend {
    fn kind(&self) -> CaptureBackendKind {
        CaptureBackendKind::PipeWire
    }

    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        // The portal decides which output is shared, and it does not tell us
        // about the others. One entry keeps the layout model happy and the
        // wizard shows the name the compositor reported once a session runs.
        Ok(vec![DisplayInfo {
            id: String::from("portal"),
            label: String::from("Shared screen (portal)"),
            width: self.size.map(|s| s.0).unwrap_or(0),
            height: self.size.map(|s| s.1).unwrap_or(0),
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
        self.stop();

        // The consent dialog blocks, so do it before the loop thread starts:
        // a failure here should surface as a plain error, not a dead thread.
        let stream = portal::open_screencast()?;
        self.size = stream.size.map(|(w, h)| (w.max(0) as u32, h.max(0) as u32));
        if let Some(label) = &stream.label {
            self.label = label.clone();
        }

        let shared = Arc::clone(&self.shared);
        let (tx, rx) = pw::channel::channel::<()>();
        shared.running.store(true, Ordering::SeqCst);
        *shared.error.lock().unwrap() = None;

        let thread = std::thread::Builder::new()
            .name(String::from("maslight-pipewire"))
            .spawn(move || {
                if let Err(e) = run_loop(Arc::clone(&shared), stream, rx) {
                    shared.fail(e);
                }
                shared.running.store(false, Ordering::SeqCst);
            })
            .map_err(|e| CaptureError::Platform(e.to_string()))?;

        self.quit = Some(tx);
        self.thread = Some(thread);
        self.seen = 0;
        Ok(())
    }

    fn next_frame(
        &mut self,
        timeout: Duration,
        on_frame: &mut dyn FnMut(&FrameView<'_>),
    ) -> Result<FrameStatus, CaptureError> {
        if self.thread.is_none() {
            return Err(CaptureError::NotStarted);
        }
        if let Some(message) = self.shared.error.lock().unwrap().clone() {
            return Err(CaptureError::Platform(message));
        }
        if !self.shared.running.load(Ordering::SeqCst) {
            return Ok(FrameStatus::Lost);
        }

        let deadline = Instant::now() + timeout;
        loop {
            let generation = self.shared.generation.load(Ordering::Acquire);
            if generation != self.seen {
                self.seen = generation;
                let slot = self.shared.slot.lock().unwrap();
                if slot.width == 0 || slot.data.is_empty() {
                    return Ok(FrameStatus::Unchanged);
                }
                let view = FrameView::new(
                    &slot.data,
                    slot.width,
                    slot.height,
                    slot.stride,
                    slot.format,
                );
                on_frame(&view);
                return Ok(FrameStatus::New);
            }
            if Instant::now() >= deadline {
                return Ok(FrameStatus::Timeout);
            }
            // The stream pushes frames at the compositor rate; a short sleep
            // is cheaper than a condition variable across an FFI boundary.
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn stop(&mut self) {
        if let Some(tx) = self.quit.take() {
            let _ = tx.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.shared.running.store(false, Ordering::SeqCst);
    }

    fn current_size(&self) -> Option<(u32, u32)> {
        let slot = self.shared.slot.lock().ok()?;
        (slot.width > 0).then_some((slot.width, slot.height))
    }
}

impl Drop for PipeWireBackend {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Map a negotiated SPA video format onto what the reducer understands.
fn pixel_format(format: VideoFormat) -> Option<PixelFormat> {
    match format {
        VideoFormat::BGRx | VideoFormat::BGRA => Some(PixelFormat::Bgra8),
        VideoFormat::RGBx | VideoFormat::RGBA => Some(PixelFormat::Rgba8),
        _ => None,
    }
}

/// State the stream callbacks share.
struct StreamState {
    shared: Arc<Shared>,
    info: VideoInfoRaw,
    format: PixelFormat,
}

fn run_loop(
    shared: Arc<Shared>,
    portal_stream: PortalStream,
    quit: pw::channel::Receiver<()>,
) -> Result<(), String> {
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| e.to_string())?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(|e| e.to_string())?;
    let core = context
        .connect_fd_rc(portal_stream.fd, None)
        .map_err(|e| format!("cannot connect to the PipeWire remote: {e}"))?;

    let props = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Video",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Screen",
    };
    let stream = pw::stream::StreamRc::new(core, "maslight", props).map_err(|e| e.to_string())?;

    let state = StreamState {
        shared: Arc::clone(&shared),
        info: VideoInfoRaw::default(),
        format: PixelFormat::Bgra8,
    };

    let _listener = stream
        .add_local_listener_with_user_data(state)
        .param_changed(|_, state, id, param| {
            let Some(param) = param else { return };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, media_subtype)) = format_utils::parse_format(param) else {
                return;
            };
            if media_type != MediaType::Video || media_subtype != MediaSubtype::Raw {
                return;
            }
            if state.info.parse(param).is_err() {
                return;
            }
            match pixel_format(state.info.format()) {
                Some(format) => {
                    state.format = format;
                    tracing::info!(
                        "PipeWire capture at {}x{} ({:?})",
                        state.info.size().width,
                        state.info.size().height,
                        state.info.format()
                    );
                }
                None => state.shared.fail(format!(
                    "unsupported pixel format {:?}",
                    state.info.format()
                )),
            }
        })
        .process(|stream, state| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }
            let data = &mut datas[0];
            let chunk_size = data.chunk().size() as usize;
            let chunk_stride = data.chunk().stride().max(0) as usize;
            let Some(pixels) = data.data() else {
                // A DMA-BUF frame arrives without a CPU mapping. Asking for
                // MAP_BUFFERS should prevent this; if it happens anyway there
                // is nothing useful to read.
                return;
            };
            if chunk_size == 0 {
                return;
            }

            let width = state.info.size().width;
            let height = state.info.size().height;
            let bpp = state.format.bytes();
            let stride = if chunk_stride > 0 {
                chunk_stride
            } else {
                width as usize * bpp
            };
            let needed = stride * height as usize;
            let available = chunk_size.min(pixels.len());
            if width == 0 || height == 0 || available < needed {
                return;
            }

            if let Ok(mut slot) = state.shared.slot.lock() {
                slot.data.clear();
                slot.data.extend_from_slice(&pixels[..needed]);
                slot.width = width;
                slot.height = height;
                slot.stride = stride;
                slot.format = state.format;
            }
            state.shared.generation.fetch_add(1, Ordering::Release);
        })
        .register()
        .map_err(|e| e.to_string())?;

    // Ask for a packed 8-bit format at whatever size the compositor offers.
    let obj = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaType,
            Id,
            MediaType::Video
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaSubtype,
            Id,
            MediaSubtype::Raw
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::RGBx,
            VideoFormat::BGRA,
            VideoFormat::RGBA
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            Rectangle {
                width: 1920,
                height: 1080
            },
            Rectangle {
                width: 16,
                height: 16
            },
            Rectangle {
                width: 8192,
                height: 8192
            }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            Fraction { num: 60, denom: 1 },
            Fraction { num: 0, denom: 1 },
            Fraction { num: 240, denom: 1 }
        ),
    );

    let values: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .map_err(|e| e.to_string())?
    .0
    .into_inner();

    let pod = Pod::from_bytes(&values).ok_or_else(|| String::from("malformed format pod"))?;
    let mut params = [pod];

    stream
        .connect(
            Direction::Input,
            Some(portal_stream.node_id),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|e| format!("cannot connect the stream: {e}"))?;

    // Stopping the backend sends on this channel, which quits the loop.
    let loop_quit = mainloop.clone();
    let _receiver = quit.attach(mainloop.loop_(), move |_| loop_quit.quit());

    mainloop.run();
    Ok(())
}
