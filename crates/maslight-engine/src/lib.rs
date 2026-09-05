//! # maslight-engine
//!
//! The loop that ties capture, colour and output together, plus the handle the
//! user interface talks to.
//!
//! The engine owns a worker thread. Everything else — the Tauri commands, the
//! tray, the local API — sends it messages and reads a status snapshot. That
//! keeps the render loop free of locks it has to wait on, so a slow UI can
//! never stutter the lights.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use maslight_capture::{create_backend, CaptureBackend, DisplayInfo, FrameStatus};
use maslight_core::{
    AppConfig, ColorPipeline, DeviceConfig, Insets, LedFrame, LightMode, Profile, Rect, Reducer,
    Rgb, Rgb8,
};
use maslight_output::{make_sink, Sink};
use parking_lot::RwLock;

mod telemetry;
pub use telemetry::{DeviceStatus, EngineStatus};

/// Messages the worker accepts.
enum Command {
    Apply(Box<AppConfig>),
    SetEnabled(bool),
    /// Light a single LED white for a while, for the calibration wizard.
    Identify {
        index: usize,
        ms: u64,
    },
    /// Drive every LED with one colour, for colour calibration.
    Hold(Option<Rgb8>),
    Shutdown,
}

/// A running engine.
pub struct EngineHandle {
    tx: Sender<Command>,
    status: Arc<RwLock<EngineStatus>>,
    alive: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl EngineHandle {
    /// Start the worker thread with an initial configuration.
    pub fn spawn(config: AppConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(RwLock::new(EngineStatus::default()));
        let alive = Arc::new(AtomicBool::new(true));

        let worker = {
            let status = Arc::clone(&status);
            let alive = Arc::clone(&alive);
            std::thread::Builder::new()
                .name(String::from("maslight-engine"))
                .spawn(move || {
                    let mut worker = Worker::new(config, status, alive);
                    worker.run(rx);
                })
                .expect("failed to start the engine thread")
        };

        Self {
            tx,
            status,
            alive,
            worker: Some(worker),
        }
    }

    /// Current telemetry. Cheap enough to poll at UI frame rate.
    pub fn status(&self) -> EngineStatus {
        self.status.read().clone()
    }

    pub fn apply(&self, config: AppConfig) {
        let _ = self.tx.send(Command::Apply(Box::new(config)));
    }

    pub fn set_enabled(&self, enabled: bool) {
        let _ = self.tx.send(Command::SetEnabled(enabled));
    }

    pub fn identify(&self, index: usize, ms: u64) {
        let _ = self.tx.send(Command::Identify { index, ms });
    }

    /// Drive the whole strip with one colour, or `None` to resume capture.
    pub fn hold_color(&self, color: Option<Rgb8>) {
        let _ = self.tx.send(Command::Hold(color));
    }

    pub fn is_running(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

/// One capture source bound to the LEDs that sample it.
struct Source {
    display: String,
    backend: Box<dyn CaptureBackend>,
    /// Sample rectangles, in the same order as `targets`.
    rects: Vec<Rect>,
    /// Index into the LED chain for each rectangle.
    targets: Vec<usize>,
    insets: Insets,
    scratch: Vec<Rgb>,
    /// Last colours, kept so an unchanged frame costs nothing.
    last: Vec<Rgb>,
    failures: u32,
}

struct Worker {
    config: AppConfig,
    status: Arc<RwLock<EngineStatus>>,
    alive: Arc<AtomicBool>,

    sources: Vec<Source>,
    sinks: Vec<Box<dyn Sink>>,
    pipeline: ColorPipeline,
    reducer: Reducer,

    /// Latest linear colour for every LED in the chain.
    canvas: Vec<Rgb>,
    /// Frames waiting out the configured latency compensation.
    delay: VecDeque<(Instant, LedFrame)>,

    hold: Option<Rgb8>,
    identify_until: Option<(usize, Instant)>,
    dirty: bool,
    last_tick: Instant,
    fps_window: VecDeque<Instant>,
    idle_frames: u32,
}

impl Worker {
    fn new(config: AppConfig, status: Arc<RwLock<EngineStatus>>, alive: Arc<AtomicBool>) -> Self {
        let mut config = config;
        config.sanitise();
        let pipeline = ColorPipeline::new(config.active().color.clone());
        let reducer = Reducer::new(config.active().capture.sample_grid);
        Self {
            config,
            status,
            alive,
            sources: Vec::new(),
            sinks: Vec::new(),
            pipeline,
            reducer,
            canvas: Vec::new(),
            delay: VecDeque::new(),
            hold: None,
            identify_until: None,
            dirty: true,
            last_tick: Instant::now(),
            fps_window: VecDeque::new(),
            idle_frames: 0,
        }
    }

    fn run(&mut self, rx: Receiver<Command>) {
        self.rebuild();
        while matches!(self.pump(&rx), ControlFlow::Continue) {
            self.tick();
        }
        self.shutdown();
    }

    /// Drain pending commands without blocking.
    fn pump(&mut self, rx: &Receiver<Command>) -> ControlFlow {
        loop {
            match rx.try_recv() {
                Ok(Command::Apply(config)) => {
                    self.config = *config;
                    self.config.sanitise();
                    self.dirty = true;
                }
                Ok(Command::SetEnabled(enabled)) => {
                    self.config.enabled = enabled;
                    self.dirty = true;
                }
                Ok(Command::Identify { index, ms }) => {
                    self.identify_until = Some((
                        index,
                        Instant::now() + Duration::from_millis(ms.min(30_000)),
                    ));
                }
                Ok(Command::Hold(color)) => {
                    self.hold = color;
                    self.pipeline.reset();
                }
                Ok(Command::Shutdown) => return ControlFlow::Stop,
                Err(TryRecvError::Empty) => return ControlFlow::Continue,
                Err(TryRecvError::Disconnected) => return ControlFlow::Stop,
            }
        }
    }

    /// Rebuild capture sources, sinks and the pipeline from the configuration.
    fn rebuild(&mut self) {
        self.dirty = false;
        let profile = self.config.active().clone();

        self.pipeline = ColorPipeline::new(profile.color.clone());
        self.reducer = Reducer::new(profile.capture.sample_grid);
        self.canvas = vec![Rgb::BLACK; profile.layout.len()];
        self.delay.clear();

        // --- outputs ---
        self.sinks.clear();
        let mut device_status = Vec::new();
        for device in &profile.devices {
            match make_sink(device, profile.layout.chain.color_order) {
                Ok(sink) => {
                    device_status.push(DeviceStatus {
                        label: sink.label(),
                        connected: true,
                        error: None,
                    });
                    self.sinks.push(sink);
                }
                Err(e) => {
                    tracing::warn!("output {} unavailable: {e}", device.label());
                    device_status.push(DeviceStatus {
                        label: device.label(),
                        connected: false,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        // --- capture ---
        self.sources.clear();
        let mut capture_error = None;
        if profile.mode == LightMode::Screen && self.config.enabled {
            for display_id in profile.layout.active_displays() {
                let mut rects = Vec::new();
                let mut targets = Vec::new();
                for led in profile.layout.leds_for_display(&display_id) {
                    rects.push(led.rect);
                    targets.push(led.index as usize);
                }
                if rects.is_empty() {
                    continue;
                }
                match create_backend(profile.capture.backend) {
                    Ok(mut backend) => {
                        // An unknown display id is not fatal: fall back to the
                        // primary one so a layout survives a monitor swap.
                        let wanted = backend.displays().ok().and_then(|list| {
                            list.iter()
                                .find(|d| d.id == display_id)
                                .map(|d| d.id.clone())
                        });
                        match backend.start(wanted.as_deref(), &profile.capture) {
                            Ok(()) => {
                                let n = rects.len();
                                self.sources.push(Source {
                                    display: display_id.clone(),
                                    backend,
                                    rects,
                                    targets,
                                    insets: Insets::default(),
                                    scratch: Vec::with_capacity(n),
                                    last: vec![Rgb::BLACK; n],
                                    failures: 0,
                                });
                            }
                            Err(e) => {
                                tracing::warn!("capture on {} failed: {e}", display_id);
                                capture_error = Some(e.to_string());
                            }
                        }
                    }
                    Err(e) => capture_error = Some(e.to_string()),
                }
            }
        }

        let backend_name = self
            .sources
            .first()
            .map(|s| format!("{:?}", s.backend.kind()))
            .unwrap_or_else(|| String::from("none"));
        let size = self
            .sources
            .first()
            .and_then(|s| s.backend.current_size())
            .unwrap_or((0, 0));

        let mut status = self.status.write();
        status.enabled = self.config.enabled;
        status.profile = profile.name.clone();
        status.profile_id = profile.id.clone();
        status.mode = profile.mode;
        status.led_count = profile.layout.len();
        status.devices = device_status;
        status.capture_backend = backend_name;
        status.display = self
            .sources
            .first()
            .map(|s| s.display.clone())
            .unwrap_or_default();
        status.source_width = size.0;
        status.source_height = size.1;
        status.last_error = capture_error;
        status.running = true;
    }

    fn tick(&mut self) {
        if self.dirty {
            self.rebuild();
        }
        let profile = self.config.active().clone();
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f32().clamp(0.0001, 1.0);
        self.last_tick = now;

        if !self.config.enabled || profile.mode == LightMode::Off {
            self.blackout(profile.layout.len());
            std::thread::sleep(Duration::from_millis(120));
            return;
        }

        // The calibration wizard takes priority over everything else.
        if let Some((index, until)) = self.identify_until {
            if now < until {
                let mut frame = LedFrame::black(profile.layout.len());
                if let Some(slot) = frame.rgb.get_mut(index) {
                    *slot = Rgb8::new(255, 255, 255);
                }
                self.emit(frame);
                std::thread::sleep(Duration::from_millis(16));
                return;
            }
            self.identify_until = None;
            self.pipeline.reset();
        }

        if let Some(color) = self.hold {
            let frame = LedFrame {
                rgb: vec![color; profile.layout.len()],
                white: Vec::new(),
            };
            self.emit(frame);
            std::thread::sleep(Duration::from_millis(33));
            return;
        }

        let target_fps = profile.capture.target_fps.max(1);
        let budget = Duration::from_secs_f32(1.0 / target_fps as f32);
        let frame_start = Instant::now();

        let mut any_new = false;
        let mut lost = false;
        for source in &mut self.sources {
            let reducer = &self.reducer;
            let detect_bars = profile.capture.detect_black_bars;
            let mut insets = source.insets;
            let mut got = false;

            let status = {
                let rects = &source.rects;
                let scratch = &mut source.scratch;
                source
                    .backend
                    .next_frame(Duration::from_millis(8), &mut |view| {
                        if detect_bars {
                            insets = maslight_core::detect_black_bars(view, 0.006);
                        }
                        reducer.reduce(view, rects, insets, scratch);
                        got = true;
                    })
            };

            match status {
                Ok(FrameStatus::New) if got => {
                    source.insets = insets;
                    source.failures = 0;
                    source.last.clone_from(&source.scratch);
                    any_new = true;
                }
                Ok(FrameStatus::Lost) => lost = true,
                Ok(_) => {}
                Err(e) => {
                    source.failures += 1;
                    if source.failures == 1 || source.failures % 240 == 0 {
                        tracing::warn!("capture error on {}: {e}", source.display);
                        self.status.write().last_error = Some(e.to_string());
                    }
                    if source.failures > 600 {
                        lost = true;
                    }
                }
            }

            for (slot, colour) in source.targets.iter().zip(source.last.iter()) {
                if let Some(c) = self.canvas.get_mut(*slot) {
                    *c = *colour;
                }
            }
        }

        if lost {
            // The session ended: a resolution change, a lock screen, or a game
            // taking the display. Rebuild rather than spinning on errors.
            tracing::info!("capture session lost, rebuilding");
            std::thread::sleep(Duration::from_millis(400));
            self.dirty = true;
            return;
        }

        if self.canvas.len() != profile.layout.len() {
            self.canvas.resize(profile.layout.len(), Rgb::BLACK);
        }

        let frame = self.pipeline.process(&self.canvas, dt);
        self.emit(frame);

        // --- pacing ---
        if any_new {
            self.idle_frames = 0;
            self.fps_window.push_back(now);
            while self
                .fps_window
                .front()
                .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(1))
            {
                self.fps_window.pop_front();
            }
        } else {
            self.idle_frames = self.idle_frames.saturating_add(1);
        }

        // When nothing has changed for a while, drop to the idle rate. This is
        // what keeps a static desktop from costing anything.
        let effective = if profile.capture.adaptive && self.idle_frames > target_fps {
            profile.capture.idle_fps.max(1)
        } else {
            target_fps
        };
        let effective_budget = Duration::from_secs_f32(1.0 / effective as f32);
        let spent = frame_start.elapsed();
        if spent < effective_budget {
            std::thread::sleep(effective_budget - spent);
        }

        let mut status = self.status.write();
        status.fps = self.fps_window.len() as f32;
        status.frame_ms = spent.as_secs_f32() * 1000.0;
        status.idle = self.idle_frames > target_fps;
        status.insets = [
            self.sources.first().map(|s| s.insets.top).unwrap_or(0.0),
            self.sources.first().map(|s| s.insets.bottom).unwrap_or(0.0),
            self.sources.first().map(|s| s.insets.left).unwrap_or(0.0),
            self.sources.first().map(|s| s.insets.right).unwrap_or(0.0),
        ];
        let _ = budget;
    }

    /// Apply latency compensation and push to every sink.
    fn emit(&mut self, frame: LedFrame) {
        let latency = self.config.active().latency_ms;
        let frame = if latency == 0 {
            frame
        } else {
            self.delay.push_back((Instant::now(), frame));
            let deadline = Duration::from_millis(latency as u64);
            let mut ready = None;
            while self
                .delay
                .front()
                .is_some_and(|(t, _)| t.elapsed() >= deadline)
            {
                ready = self.delay.pop_front().map(|(_, f)| f);
            }
            // Guard against unbounded growth if the clock jumps.
            while self.delay.len() > 240 {
                self.delay.pop_front();
            }
            match ready {
                Some(f) => f,
                None => return,
            }
        };

        for sink in &mut self.sinks {
            if let Err(e) = sink.push(&frame) {
                tracing::debug!("sink {} failed: {e}", sink.label());
                let label = sink.label();
                let mut status = self.status.write();
                if let Some(d) = status.devices.iter_mut().find(|d| d.label == label) {
                    d.connected = false;
                    d.error = Some(e.to_string());
                }
            }
        }

        let mut status = self.status.write();
        status.leds = frame.rgb.clone();
        status.frames = status.frames.wrapping_add(1);
    }

    fn blackout(&mut self, len: usize) {
        for sink in &mut self.sinks {
            let _ = sink.blackout(len);
        }
        let mut status = self.status.write();
        status.leds = vec![Rgb8::BLACK; len];
        status.fps = 0.0;
    }

    fn shutdown(&mut self) {
        let len = self.config.active().layout.len();
        self.blackout(len);
        for source in &mut self.sources {
            source.backend.stop();
        }
        self.sources.clear();
        self.sinks.clear();
        self.alive.store(false, Ordering::Relaxed);
        self.status.write().running = false;
    }
}

enum ControlFlow {
    Continue,
    Stop,
}

/// Displays visible to a capture backend, for the setup wizard.
pub fn list_displays(kind: maslight_core::CaptureBackendKind) -> Vec<DisplayInfo> {
    create_backend(kind)
        .and_then(|b| b.displays())
        .unwrap_or_default()
}

/// Devices found on the local network.
pub fn discover_devices(timeout_ms: u64) -> Vec<maslight_output::discovery::DiscoveredDevice> {
    maslight_output::discovery::discover_wled(Duration::from_millis(timeout_ms))
}

/// Build a device entry and a matching profile for a freshly discovered
/// controller, so the wizard can go from "found it" to "lights on" in one step.
pub fn profile_for_discovered(
    device: &maslight_output::discovery::DiscoveredDevice,
    display: &str,
) -> Profile {
    let led_count = device.led_count.unwrap_or(60);
    // Split the strip around the screen in the proportions of a 16:9 panel.
    let horizontal = ((led_count as f32 * 0.32).round() as u32).max(1);
    let vertical = ((led_count as f32 * 0.18).round() as u32).max(1);
    let layout = maslight_core::Layout::from_wizard(&maslight_core::WizardParams {
        name: device.name.clone(),
        display: display.to_string(),
        counts: maslight_core::EdgeCounts {
            top: horizontal,
            bottom: horizontal,
            left: vertical,
            right: vertical,
        },
        ..Default::default()
    });
    Profile {
        id: format!("wled-{}", device.host),
        name: device.name.clone(),
        layout,
        devices: vec![DeviceConfig::Wled {
            host: device.host.clone(),
            port: 21324,
            protocol: maslight_output::wled::protocol_for(led_count as usize, false),
            timeout_s: 2,
        }],
        ..Default::default()
    }
}
