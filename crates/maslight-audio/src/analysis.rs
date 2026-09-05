//! Turning a window of samples into something an LED strip can show.
//!
//! The chain is short on purpose: window, FFT, group into log spaced bands,
//! convert to decibels, normalise against a floor and a ceiling, then smooth
//! with separate attack and release. Everything after the FFT is per band and
//! costs nothing.
//!
//! Log spacing matters. Music has most of its energy in the bottom two
//! octaves, so linear bands give you a strip where the first two LEDs move and
//! the rest sit still.

use std::sync::Arc;

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

/// Samples per analysis window. At 48 kHz this is 21 ms, which resolves down
/// to about 47 Hz: low enough for a bass line, short enough to feel immediate.
pub const WINDOW: usize = 1024;

/// Smallest and largest frequency the bands cover.
const LOW_HZ: f32 = 40.0;
const HIGH_HZ: f32 = 16_000.0;

/// One analysis pass over a window of mono samples.
pub struct Analyser {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    scratch: Vec<Complex32>,
    /// Band edges as FFT bin indices, `bands + 1` entries.
    edges: Vec<usize>,
    /// Smoothed band levels, 0..=1.
    levels: Vec<f32>,
    /// Smoothed broadband loudness, 0..=1.
    energy: f32,
    /// Running mean and variance of low band energy, for beat detection.
    flux_mean: f32,
    flux_var: f32,
    last_low: f32,
    beat_hold: f32,
    sample_rate: f32,
}

impl std::fmt::Debug for Analyser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Analyser")
            .field("bands", &self.levels.len())
            .field("sample_rate", &self.sample_rate)
            .finish()
    }
}

/// What one analysis pass produced.
#[derive(Clone, Debug, Default)]
pub struct Spectrum {
    /// Per band level, 0..=1, already smoothed.
    pub bands: Vec<f32>,
    /// Overall loudness, 0..=1.
    pub energy: f32,
    /// True on the frame a beat was detected.
    pub beat: bool,
    /// Decays from 1.0 to 0.0 after each beat, for effects that want a pulse.
    pub beat_envelope: f32,
}

impl Analyser {
    pub fn new(sample_rate: f32, bands: usize) -> Self {
        let bands = bands.clamp(2, 64);
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(WINDOW);

        // Hann window. Without it a tone that does not land exactly on a bin
        // smears across the whole spectrum and every band lights up.
        let window = (0..WINDOW)
            .map(|i| {
                let t = i as f32 / (WINDOW - 1) as f32;
                0.5 - 0.5 * (std::f32::consts::TAU * t).cos()
            })
            .collect();

        Self {
            fft,
            window,
            scratch: vec![Complex32::new(0.0, 0.0); WINDOW],
            edges: band_edges(sample_rate, bands),
            levels: vec![0.0; bands],
            energy: 0.0,
            flux_mean: 0.0,
            flux_var: 0.0,
            last_low: 0.0,
            beat_hold: 0.0,
            sample_rate,
        }
    }

    pub fn bands(&self) -> usize {
        self.levels.len()
    }

    /// Rebuild for a different band count or sample rate.
    pub fn reconfigure(&mut self, sample_rate: f32, bands: usize) {
        if (sample_rate - self.sample_rate).abs() < 1.0 && bands == self.levels.len() {
            return;
        }
        *self = Self::new(sample_rate, bands);
    }

    /// Analyse one window.
    ///
    /// `samples` must be [`WINDOW`] long. `dt` is the time since the previous
    /// call, so smoothing stays frame rate independent.
    pub fn process(
        &mut self,
        samples: &[f32],
        settings: &crate::AudioSettings,
        dt: f32,
    ) -> Spectrum {
        let n = self.levels.len();
        if samples.len() < WINDOW {
            return Spectrum {
                bands: vec![0.0; n],
                energy: 0.0,
                beat: false,
                beat_envelope: self.beat_hold,
            };
        }

        for (i, slot) in self.scratch.iter_mut().enumerate() {
            *slot = Complex32::new(samples[i] * self.window[i], 0.0);
        }
        self.fft.process(&mut self.scratch);

        // Magnitude of the first half; the rest mirrors it.
        let half = WINDOW / 2;
        let norm = 2.0 / WINDOW as f32;

        let attack = coefficient(settings.attack_ms, dt);
        let release = coefficient(settings.release_ms, dt);
        let floor = settings.floor_db;
        let ceiling = settings.ceiling_db.max(floor + 1.0);

        let mut raw = vec![0.0f32; n];
        for (band, slot) in raw.iter_mut().enumerate() {
            let start = self.edges[band].min(half.saturating_sub(1));
            let end = self.edges[band + 1].clamp(start + 1, half);
            let sum: f32 = self.scratch[start..end]
                .iter()
                .map(|c| c.norm() * norm)
                .sum();
            let mean = sum / (end - start) as f32;
            // Decibels, then a straight line between the floor and ceiling.
            let db = 20.0 * (mean.max(1e-7)).log10();
            *slot = ((db - floor) / (ceiling - floor)).clamp(0.0, 1.0);
        }

        if settings.gain != 1.0 {
            for v in &mut raw {
                *v = (*v * settings.gain).clamp(0.0, 1.0);
            }
        }

        for (level, target) in self.levels.iter_mut().zip(raw.iter()) {
            // Rise fast, fall slow: this is what makes a spectrum look alive
            // rather than jittery.
            let k = if *target > *level { attack } else { release };
            *level += (*target - *level) * k;
        }

        // Beat detection on the bottom quarter of the spectrum. Positive flux
        // against an adaptive threshold is simple and works on real music.
        let low_count = (n / 4).max(1);
        let low: f32 = self.levels[..low_count].iter().sum::<f32>() / low_count as f32;
        let flux = (low - self.last_low).max(0.0);
        self.last_low = low;

        let alpha = 0.08;
        self.flux_mean += (flux - self.flux_mean) * alpha;
        let deviation = (flux - self.flux_mean).powi(2);
        self.flux_var += (deviation - self.flux_var) * alpha;
        let threshold = self.flux_mean + settings.beat_sensitivity * self.flux_var.sqrt();

        let beat = flux > threshold && flux > 0.008 && self.beat_hold < 0.4;
        if beat {
            self.beat_hold = 1.0;
        } else {
            // About a 250 ms decay, which reads as a pulse rather than a flash.
            self.beat_hold = (self.beat_hold - dt * 4.0).max(0.0);
        }

        // Loudness is measured across the whole window rather than as a mean
        // of the bands. A pure tone fills one band out of sixteen, so a mean
        // would call a loud sine quiet, which is not what anyone hears.
        let mean_square: f32 = samples[..WINDOW].iter().map(|s| s * s).sum::<f32>() / WINDOW as f32;
        let rms_db = 20.0 * mean_square.sqrt().max(1e-7).log10();
        // Broadband RMS sits well above any single band mean, so loudness
        // needs its own headroom or it pins at full on ordinary music.
        let energy_ceiling = ceiling + 8.0;
        let energy_target = ((rms_db - floor) / (energy_ceiling - floor)).clamp(0.0, 1.0)
            * settings.gain.clamp(0.1, 8.0);
        let energy_target = energy_target.clamp(0.0, 1.0);
        let k = if energy_target > self.energy {
            attack
        } else {
            release
        };
        self.energy += (energy_target - self.energy) * k;

        Spectrum {
            bands: self.levels.clone(),
            energy: self.energy,
            beat,
            beat_envelope: self.beat_hold,
        }
    }
}

/// One pole coefficient for a time constant in milliseconds.
fn coefficient(ms: f32, dt: f32) -> f32 {
    if ms <= 0.0 || dt <= 0.0 {
        return 1.0;
    }
    (1.0 - (-dt / (ms / 1000.0)).exp()).clamp(0.0, 1.0)
}

/// Band edges as FFT bin indices, spaced logarithmically.
fn band_edges(sample_rate: f32, bands: usize) -> Vec<usize> {
    let bin_hz = sample_rate / WINDOW as f32;
    let low = LOW_HZ.ln();
    let high = HIGH_HZ
        .min(sample_rate / 2.0 - bin_hz)
        .max(LOW_HZ * 2.0)
        .ln();

    let mut edges = Vec::with_capacity(bands + 1);
    for i in 0..=bands {
        let t = i as f32 / bands as f32;
        let hz = (low + (high - low) * t).exp();
        let bin = (hz / bin_hz).round() as usize;
        // Every band has to own at least one bin, or it stays dark forever.
        let bin = bin.max(edges.last().map(|p| p + 1).unwrap_or(1));
        edges.push(bin.min(WINDOW / 2));
    }
    edges
}
