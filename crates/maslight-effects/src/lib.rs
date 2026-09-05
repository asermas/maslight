//! # maslight-effects
//!
//! Scripted effects, sandboxed.
//!
//! A script is a small Rhai program with one entry point:
//!
//! ```text
//! fn render(ctx) {
//!     let out = [];
//!     for i in 0..ctx.n {
//!         let h = (i.to_float() / ctx.n.to_float() + ctx.t * 0.1) % 1.0;
//!         out.push(hsv(h, 1.0, 1.0));
//!     }
//!     out
//! }
//! ```
//!
//! It returns one colour per LED. Colours are `[r, g, b]` in **linear light**,
//! `0.0` to `1.0`, because everything downstream of here is linear: the script
//! sits exactly where the screen or the audio engine would, and the colour
//! pipeline applies brightness, white balance and gamma to the result once.
//!
//! ## The sandbox
//!
//! Rhai has no file, network or process access to begin with, and this engine
//! removes what is left:
//!
//! * One `render` call is capped at a fixed number of operations, so an
//!   accidental infinite loop stops the script rather than the lights.
//! * Recursion, array size, string size and expression depth are all bounded.
//! * A script that fails is reported and its LEDs go dark; it never takes the
//!   engine with it.
//!
//! The cap is per call rather than per second, which means a script cannot
//! borrow time from a quiet frame to hang a busy one.

use std::time::Instant;

use maslight_core::Rgb;
use rhai::{Array, Dynamic, Engine, Map, Scope, AST};

/// Operations one `render` call may execute.
///
/// Sixty LEDs of ordinary trigonometry is a few thousand; a hundred thousand
/// leaves room for something ambitious and still stops a runaway loop inside a
/// single frame.
const MAX_OPERATIONS: u64 = 100_000;

/// What a script can see about the moment it is drawing.
#[derive(Clone, Debug, Default)]
pub struct EffectContext {
    /// Number of LEDs in the chain.
    pub n: usize,
    /// Seconds since the effect started.
    pub t: f32,
    /// Seconds since the previous frame.
    pub dt: f32,
    /// Overall audio loudness, 0..=1. Zero when audio is not running.
    pub energy: f32,
    /// True on the frame a beat was detected.
    pub beat: bool,
    /// Decays from 1.0 after each beat.
    pub beat_envelope: f32,
    /// Per band audio levels, 0..=1. Empty when audio is not running.
    pub bands: Vec<f32>,
}

/// Why a script did not run.
#[derive(Debug, thiserror::Error)]
pub enum EffectError {
    #[error("the script did not compile: {0}")]
    Compile(String),
    #[error("the script failed: {0}")]
    Runtime(String),
    #[error("the script has no render function")]
    NoRender,
    #[error("render returned {0}, expected an array of colours")]
    BadReturn(String),
}

/// A compiled script, ready to run every frame.
pub struct EffectScript {
    engine: Engine,
    ast: AST,
    source: String,
    started: Instant,
    /// Set when a frame fails, so the failure is reported once rather than on
    /// every frame.
    last_error: Option<String>,
}

impl std::fmt::Debug for EffectScript {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectScript")
            .field("bytes", &self.source.len())
            .field("error", &self.last_error)
            .finish()
    }
}

impl EffectScript {
    /// Compile a script.
    pub fn compile(source: &str) -> Result<Self, EffectError> {
        let engine = sandbox();
        let ast = engine
            .compile(source)
            .map_err(|e| EffectError::Compile(e.to_string()))?;
        if !ast.iter_functions().any(|f| f.name == "render") {
            return Err(EffectError::NoRender);
        }
        Ok(Self {
            engine,
            ast,
            source: source.to_string(),
            started: Instant::now(),
            last_error: None,
        })
    }

    /// The source this was compiled from.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Restart the clock the script sees.
    pub fn reset(&mut self) {
        self.started = Instant::now();
        self.last_error = None;
    }

    /// The most recent runtime failure, if any.
    pub fn error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Run one frame, filling `out` with one colour per LED.
    ///
    /// On failure the output is black and the error is stored rather than
    /// returned every frame: a broken script should be visible in the
    /// interface, not spam the log sixty times a second.
    pub fn render(&mut self, context: &EffectContext, out: &mut Vec<Rgb>) -> bool {
        out.clear();
        let n = context.n;
        if n == 0 {
            return true;
        }

        let mut ctx = Map::new();
        ctx.insert("n".into(), Dynamic::from(n as i64));
        ctx.insert(
            "t".into(),
            Dynamic::from(self.started.elapsed().as_secs_f32()),
        );
        ctx.insert("dt".into(), Dynamic::from(context.dt));
        ctx.insert("energy".into(), Dynamic::from(context.energy));
        ctx.insert("beat".into(), Dynamic::from(context.beat));
        ctx.insert("pulse".into(), Dynamic::from(context.beat_envelope));
        ctx.insert(
            "bands".into(),
            Dynamic::from(
                context
                    .bands
                    .iter()
                    .map(|v| Dynamic::from(*v))
                    .collect::<Array>(),
            ),
        );

        let mut scope = Scope::new();
        let result: Result<Array, _> =
            self.engine
                .call_fn(&mut scope, &self.ast, "render", (Dynamic::from(ctx),));

        let array = match result {
            Ok(array) => array,
            Err(e) => {
                self.fail(e.to_string(), n, out);
                return false;
            }
        };

        for value in array.into_iter().take(n) {
            out.push(to_rgb(&value));
        }
        // A script that returns a short array leaves the rest dark rather than
        // repeating: repeating would hide the mistake.
        while out.len() < n {
            out.push(Rgb::BLACK);
        }
        self.last_error = None;
        true
    }

    fn fail(&mut self, message: String, n: usize, out: &mut Vec<Rgb>) {
        if self.last_error.as_deref() != Some(message.as_str()) {
            tracing::warn!("effect script failed: {message}");
        }
        self.last_error = Some(message);
        out.clear();
        out.extend(std::iter::repeat_n(Rgb::BLACK, n));
    }
}

/// Turn a script value into a colour.
///
/// Accepts `[r, g, b]` and a bare number for grey, because both read naturally
/// in a short script and guessing wrong is worse than accepting both.
fn to_rgb(value: &Dynamic) -> Rgb {
    if let Some(array) = value.read_lock::<Array>() {
        let get = |i: usize| -> f32 {
            array
                .get(i)
                .and_then(|v| v.as_float().ok())
                .unwrap_or(0.0)
                .clamp(0.0, 1.0)
        };
        return Rgb::new(get(0), get(1), get(2));
    }
    if let Ok(v) = value.as_float() {
        let v = v.clamp(0.0, 1.0);
        return Rgb::new(v, v, v);
    }
    if let Ok(v) = value.as_int() {
        let v = (v as f32).clamp(0.0, 1.0);
        return Rgb::new(v, v, v);
    }
    Rgb::BLACK
}

/// An engine a script cannot escape from.
fn sandbox() -> Engine {
    let mut engine = Engine::new_raw();
    engine.set_max_operations(MAX_OPERATIONS);
    engine.set_max_call_levels(24);
    engine.set_max_array_size(8192);
    engine.set_max_map_size(256);
    engine.set_max_string_size(4096);
    engine.set_max_expr_depths(64, 32);

    // `eval` is part of the language rather than a package, so it survives a
    // raw engine. A script that can build code at runtime cannot be reviewed
    // by reading it, which is the whole point of sharing scripts as text.
    engine.disable_symbol("eval");

    // A raw engine has no standard library at all, so everything a script can
    // reach is on this list and nothing else.
    engine.register_global_module(rhai::packages::Package::as_shared_module(
        &rhai::packages::BasicArrayPackage::new(),
    ));
    engine.register_global_module(rhai::packages::Package::as_shared_module(
        &rhai::packages::BasicMathPackage::new(),
    ));
    engine.register_global_module(rhai::packages::Package::as_shared_module(
        &rhai::packages::ArithmeticPackage::new(),
    ));
    engine.register_global_module(rhai::packages::Package::as_shared_module(
        &rhai::packages::LogicPackage::new(),
    ));
    engine.register_global_module(rhai::packages::Package::as_shared_module(
        &rhai::packages::BasicIteratorPackage::new(),
    ));

    // Colour helpers, so a script does not have to reimplement HSV.
    engine.register_fn("hsv", |h: f32, s: f32, v: f32| -> Array {
        let c = hsv(h, s, v);
        vec![Dynamic::from(c.r), Dynamic::from(c.g), Dynamic::from(c.b)]
    });
    engine.register_fn("rgb", |r: f32, g: f32, b: f32| -> Array {
        vec![
            Dynamic::from(r.clamp(0.0, 1.0)),
            Dynamic::from(g.clamp(0.0, 1.0)),
            Dynamic::from(b.clamp(0.0, 1.0)),
        ]
    });
    engine.register_fn("clamp01", |v: f32| v.clamp(0.0, 1.0));
    engine.register_fn("mix", |a: f32, b: f32, t: f32| {
        a + (b - a) * t.clamp(0.0, 1.0)
    });
    engine.register_fn("wave", |t: f32| {
        (t * std::f32::consts::TAU).sin() * 0.5 + 0.5
    });

    engine
}

/// HSV to linear light RGB, the same one the audio effects use.
pub fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h = h.rem_euclid(1.0) * 6.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i.rem_euclid(6) {
        0 => Rgb::new(v, t, p),
        1 => Rgb::new(q, v, p),
        2 => Rgb::new(p, v, t),
        3 => Rgb::new(p, q, v),
        4 => Rgb::new(t, p, v),
        _ => Rgb::new(v, p, q),
    }
}

/// Scripts shipped with the app, as a starting point.
pub fn examples() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Rainbow",
            r#"// A hue that walks along the strip and drifts with time.
fn render(ctx) {
    let out = [];
    for i in 0..ctx.n {
        let h = i.to_float() / ctx.n.to_float() + ctx.t * 0.08;
        out.push(hsv(h, 1.0, 1.0));
    }
    out
}"#,
        ),
        (
            "Breathing",
            r#"// One colour, rising and falling once every four seconds.
fn render(ctx) {
    let v = wave(ctx.t * 0.25) * 0.9 + 0.1;
    let out = [];
    for i in 0..ctx.n {
        out.push(hsv(0.08, 0.85, v));
    }
    out
}"#,
        ),
        (
            "Beat pulse",
            r#"// Dark until the music hits, then a flash that fades.
// Needs the audio mode blended in, or ctx.pulse stays at zero.
fn render(ctx) {
    let out = [];
    for i in 0..ctx.n {
        let along = i.to_float() / ctx.n.to_float();
        let v = clamp01(ctx.pulse - (along - 0.5).abs());
        out.push(hsv(0.55 + ctx.energy * 0.2, 1.0, v));
    }
    out
}"#,
        ),
        (
            "Meter",
            r#"// The strip as a level meter: green up to the level, dark above it.
fn render(ctx) {
    let out = [];
    for i in 0..ctx.n {
        let along = i.to_float() / ctx.n.to_float();
        if along < ctx.energy {
            out.push(hsv(mix(0.33, 0.0, along), 1.0, 1.0));
        } else {
            out.push(rgb(0.0, 0.0, 0.0));
        }
    }
    out
}"#,
        ),
    ]
}
