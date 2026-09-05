//! # maslight-audio
//!
//! System audio in, colours out.
//!
//! Capture is loopback: MasLight listens to what the speakers are playing
//! rather than to a microphone, so it reacts to whatever is actually being
//! played without any routing. On Windows that is WASAPI loopback, which cpal
//! enables automatically when an input stream is opened on an output device.
//! On Linux the same code picks a PipeWire or PulseAudio monitor source.
//!
//! The capture callback does nothing but copy samples into a ring buffer. All
//! the analysis happens on the engine thread, so a slow frame can never stall
//! the audio device.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use maslight_core::Rgb;
use parking_lot::Mutex;

pub mod analysis;
pub mod effects;

pub use analysis::{Analyser, Spectrum, WINDOW};
pub use maslight_core::audio::{AudioEffect, AudioSettings, Palette};

/// Failures the audio path can report.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no audio device is available")]
    NoDevice,
    #[error("the audio device refused the request: {0}")]
    Device(String),
    #[error("audio capture has not been started")]
    NotStarted,
}

/// A lock free enough ring of recent mono samples.
///
/// The capture callback is the only writer and it only ever copies, so the
/// lock is held for microseconds.
struct Ring {
    data: Vec<f32>,
    write: usize,
    filled: bool,
}

impl Ring {
    fn new(len: usize) -> Self {
        Self {
            data: vec![0.0; len],
            write: 0,
            filled: false,
        }
    }

    fn push(&mut self, sample: f32) {
        self.data[self.write] = sample;
        self.write = (self.write + 1) % self.data.len();
        if self.write == 0 {
            self.filled = true;
        }
    }

    /// Copy the most recent `out.len()` samples in order.
    fn read_latest(&self, out: &mut [f32]) -> bool {
        let n = out.len().min(self.data.len());
        if !self.filled && self.write < n {
            return false;
        }
        for (i, slot) in out.iter_mut().enumerate() {
            let idx = (self.write + self.data.len() - n + i) % self.data.len();
            *slot = self.data[idx];
        }
        true
    }
}

/// Loopback capture plus analysis.
pub struct AudioEngine {
    ring: Arc<Mutex<Ring>>,
    stream: Option<cpal::Stream>,
    analyser: Analyser,
    sample_rate: f32,
    device_name: String,
    scratch: Vec<f32>,
    phase: f32,
    last: Spectrum,
}

impl AudioEngine {
    /// Open the default output device in loopback mode.
    pub fn start(settings: &AudioSettings) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = match &settings.device {
            Some(name) => find_device(&host, name).ok_or(AudioError::NoDevice)?,
            None => default_loopback(&host).ok_or(AudioError::NoDevice)?,
        };
        // cpal 0.18 exposes the human name through Display rather than a method.
        let device_name = device.to_string();

        // An output device used as an input is loopback; a monitor source is
        // already an input. Ask for whichever config the device offers.
        let config = device
            .default_output_config()
            .or_else(|_| device.default_input_config())
            .map_err(|e| AudioError::Device(e.to_string()))?;
        let sample_rate = config.sample_rate() as f32;
        let channels = config.channels() as usize;

        // Two windows of history is enough to always have a full window ready
        // without letting stale audio pile up.
        let ring = Arc::new(Mutex::new(Ring::new(WINDOW * 2)));
        let writer = Arc::clone(&ring);

        let stream = device
            .build_input_stream(
                config.config(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut ring = writer.lock();
                    // Downmix to mono. Averaging is right here: a spectrum of
                    // one channel would miss anything panned to the other.
                    for frame in data.chunks(channels.max(1)) {
                        let sum: f32 = frame.iter().sum();
                        ring.push(sum / frame.len().max(1) as f32);
                    }
                },
                |err| tracing::warn!("audio stream error: {err}"),
                None,
            )
            .map_err(|e| AudioError::Device(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::Device(e.to_string()))?;

        tracing::info!(
            "audio capture on {device_name} at {} Hz, {} channels",
            sample_rate,
            channels
        );

        Ok(Self {
            ring,
            stream: Some(stream),
            analyser: Analyser::new(sample_rate, settings.bands as usize),
            sample_rate,
            device_name,
            scratch: vec![0.0; WINDOW],
            phase: 0.0,
            last: Spectrum::default(),
        })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn spectrum(&self) -> &Spectrum {
        &self.last
    }

    /// Analyse the newest window and render one frame of `len` LEDs.
    pub fn frame(
        &mut self,
        settings: &AudioSettings,
        len: usize,
        dt: f32,
        out: &mut Vec<Rgb>,
    ) -> bool {
        if self.stream.is_none() {
            out.clear();
            out.extend(std::iter::repeat_n(Rgb::BLACK, len));
            return false;
        }
        self.analyser
            .reconfigure(self.sample_rate, settings.bands as usize);

        let have = self.ring.lock().read_latest(&mut self.scratch);
        if !have {
            out.clear();
            out.extend(std::iter::repeat_n(Rgb::BLACK, len));
            return false;
        }

        self.last = self.analyser.process(&self.scratch, settings, dt);
        self.phase = (self.phase + settings.scroll_speed * dt).rem_euclid(1.0);
        effects::render(
            settings.effect,
            &self.last,
            &settings.palette,
            settings.beat_boost,
            self.phase,
            len,
            out,
        );
        true
    }

    pub fn stop(&mut self) {
        self.stream = None;
    }
}

impl std::fmt::Debug for AudioEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioEngine")
            .field("device", &self.device_name)
            .field("sample_rate", &self.sample_rate)
            .finish()
    }
}

/// The device to listen to when the profile does not name one.
fn default_loopback(host: &cpal::Host) -> Option<cpal::Device> {
    // Windows: an output device opened for input is loopback.
    if cfg!(target_os = "windows") {
        return host.default_output_device();
    }
    // Linux and macOS: look for a monitor source, then fall back to whatever
    // the system considers the default input.
    if let Ok(devices) = host.input_devices() {
        for device in devices {
            let lower = device.to_string().to_lowercase();
            if lower.contains("monitor") || lower.contains("loopback") {
                return Some(device);
            }
        }
    }
    host.default_input_device()
}

fn find_device(host: &cpal::Host, wanted: &str) -> Option<cpal::Device> {
    let matches = |device: &cpal::Device| device.to_string() == wanted;
    if let Ok(mut devices) = host.input_devices() {
        if let Some(device) = devices.find(&matches) {
            return Some(device);
        }
    }
    if let Ok(mut devices) = host.output_devices() {
        if let Some(device) = devices.find(&matches) {
            return Some(device);
        }
    }
    None
}

/// Devices the audio mode can listen to, for the settings screen.
pub fn list_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut names = Vec::new();
    if let Ok(devices) = host.output_devices() {
        for device in devices {
            names.push(device.to_string());
        }
    }
    if let Ok(devices) = host.input_devices() {
        for device in devices {
            let name = device.to_string();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}
