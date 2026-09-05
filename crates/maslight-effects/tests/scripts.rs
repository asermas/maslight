use maslight_core::Rgb;
use maslight_effects::{EffectContext, EffectError, EffectScript};

fn context(n: usize) -> EffectContext {
    EffectContext {
        n,
        t: 0.0,
        dt: 1.0 / 60.0,
        energy: 0.5,
        beat: false,
        beat_envelope: 0.0,
        bands: vec![0.25; 16],
    }
}

fn run(source: &str, n: usize) -> (Vec<Rgb>, Option<String>) {
    let mut script = EffectScript::compile(source).expect("should compile");
    let mut out = Vec::new();
    script.render(&context(n), &mut out);
    (out, script.error().map(str::to_string))
}

#[test]
fn every_shipped_example_compiles_and_lights_something() {
    for (name, source) in maslight_effects::examples() {
        let mut script =
            EffectScript::compile(source).unwrap_or_else(|e| panic!("{name} did not compile: {e}"));
        let mut out = Vec::new();
        let mut ctx = context(32);
        // Give the beat driven ones something to react to.
        ctx.energy = 0.8;
        ctx.beat_envelope = 1.0;
        assert!(script.render(&ctx, &mut out), "{name} failed to render");
        assert_eq!(out.len(), 32, "{name} produced the wrong length");
        assert!(
            out.iter().any(|c| c.max_channel() > 0.05),
            "{name} produced nothing visible"
        );
        assert!(script.error().is_none(), "{name}: {:?}", script.error());
    }
}

#[test]
fn a_script_fills_the_whole_chain() {
    let (out, error) = run(
        r#"fn render(ctx) {
            let out = [];
            for i in 0..ctx.n { out.push(rgb(1.0, 0.0, 0.0)); }
            out
        }"#,
        20,
    );
    assert_eq!(error, None);
    assert_eq!(out.len(), 20);
    assert!(out.iter().all(|c| c.r > 0.99 && c.g < 0.01));
}

#[test]
fn a_short_return_leaves_the_rest_dark_rather_than_repeating() {
    // Repeating would quietly hide the mistake.
    let (out, _) = run(
        r#"fn render(ctx) { [rgb(1.0, 1.0, 1.0), rgb(1.0, 1.0, 1.0)] }"#,
        8,
    );
    assert_eq!(out.len(), 8);
    assert!(out[0].max_channel() > 0.9);
    assert!(out[2..].iter().all(|c| *c == Rgb::BLACK));
}

#[test]
fn a_long_return_is_truncated() {
    let (out, _) = run(
        r#"fn render(ctx) {
            let out = [];
            for i in 0..100 { out.push(rgb(0.5, 0.5, 0.5)); }
            out
        }"#,
        4,
    );
    assert_eq!(out.len(), 4);
}

#[test]
fn colours_are_clamped_into_range() {
    let (out, _) = run(r#"fn render(ctx) { [rgb(5.0, -3.0, 0.5)] }"#, 1);
    assert_eq!(out[0].r, 1.0);
    assert_eq!(out[0].g, 0.0);
    assert!((out[0].b - 0.5).abs() < 1e-6);
}

#[test]
fn a_bare_number_is_read_as_grey() {
    let (out, _) = run(r#"fn render(ctx) { [0.5, 1.0] }"#, 2);
    assert!((out[0].r - 0.5).abs() < 1e-6);
    assert_eq!(out[0].r, out[0].g);
    assert_eq!(out[1].r, 1.0);
}

#[test]
fn the_context_carries_the_audio_state() {
    let (out, error) = run(
        r#"fn render(ctx) {
            let out = [];
            for i in 0..ctx.n { out.push(rgb(ctx.energy, ctx.bands[0], 0.0)); }
            out
        }"#,
        4,
    );
    assert_eq!(error, None);
    assert!(
        (out[0].r - 0.5).abs() < 1e-5,
        "energy should reach the script"
    );
    assert!((out[0].g - 0.25).abs() < 1e-5, "bands should reach it too");
}

#[test]
fn a_script_without_render_is_refused_at_compile_time() {
    let error = EffectScript::compile("fn other() { 1 }").unwrap_err();
    assert!(matches!(error, EffectError::NoRender), "{error}");
}

#[test]
fn a_syntax_error_is_reported_rather_than_panicking() {
    let error = EffectScript::compile("fn render(ctx) { this is not rhai }").unwrap_err();
    assert!(matches!(error, EffectError::Compile(_)), "{error}");
}

#[test]
fn an_infinite_loop_stops_the_script_not_the_lights() {
    // This is the property that makes scripting safe to expose: a mistake in a
    // script costs one dark frame, never the engine.
    let mut script = EffectScript::compile(
        r#"fn render(ctx) {
            let x = 0;
            loop { x += 1; }
            []
        }"#,
    )
    .expect("should compile");

    let mut out = Vec::new();
    let started = std::time::Instant::now();
    let ok = script.render(&context(16), &mut out);
    let elapsed = started.elapsed();

    assert!(!ok, "a runaway script should report failure");
    assert!(script.error().is_some());
    assert_eq!(out.len(), 16, "the chain still has to be filled");
    assert!(out.iter().all(|c| *c == Rgb::BLACK), "and it goes dark");
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "the operation cap should stop it quickly, took {elapsed:?}"
    );
}

#[test]
fn a_runtime_error_recovers_on_the_next_good_frame() {
    let mut script = EffectScript::compile(
        r#"fn render(ctx) {
            if ctx.n == 0 { throw "no leds"; }
            let out = [];
            for i in 0..ctx.n { out.push(rgb(0.0, 1.0, 0.0)); }
            out
        }"#,
    )
    .unwrap();

    let mut out = Vec::new();
    assert!(!script.render(&context(0), &mut out) || out.is_empty());

    assert!(script.render(&context(4), &mut out));
    assert_eq!(script.error(), None, "a good frame clears the error");
    assert!(out.iter().all(|c| c.g > 0.9));
}

#[test]
fn a_script_cannot_reach_the_file_system() {
    // The engine is raw with a hand picked set of packages, so anything that
    // touches the outside world is simply not a function.
    for attempt in [
        r#"fn render(ctx) { open_file("secret.txt"); [] }"#,
        r#"fn render(ctx) { import "std" as s; [] }"#,
        r#"fn render(ctx) { eval("1 + 1"); [] }"#,
        r#"fn render(ctx) { print("hello"); [] }"#,
    ] {
        let result = EffectScript::compile(attempt);
        let failed = match result {
            Err(_) => true,
            Ok(mut script) => {
                let mut out = Vec::new();
                !script.render(&context(4), &mut out)
            }
        };
        assert!(failed, "this should not have worked: {attempt}");
    }
}

#[test]
fn a_huge_allocation_is_refused() {
    let mut script = EffectScript::compile(
        r#"fn render(ctx) {
            let out = [];
            for i in 0..1000000 { out.push(0.5); }
            out
        }"#,
    )
    .unwrap();
    let mut out = Vec::new();
    assert!(
        !script.render(&context(8), &mut out),
        "the array cap or the operation cap has to stop this"
    );
    assert_eq!(out.len(), 8);
}

#[test]
fn the_clock_advances_between_frames() {
    let mut script = EffectScript::compile(
        r#"fn render(ctx) {
            let out = [];
            for i in 0..ctx.n { out.push(clamp01(ctx.t)); }
            out
        }"#,
    )
    .unwrap();

    let mut first = Vec::new();
    script.render(&context(2), &mut first);
    std::thread::sleep(std::time::Duration::from_millis(40));
    let mut second = Vec::new();
    script.render(&context(2), &mut second);

    assert!(
        second[0].r > first[0].r,
        "t should move: {} then {}",
        first[0].r,
        second[0].r
    );
}

#[test]
fn an_empty_chain_is_handled() {
    let (out, _) = run(r#"fn render(ctx) { [] }"#, 0);
    assert!(out.is_empty());
}
