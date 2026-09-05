---
title: Scripting
---

# Scripted effects

The **Developer** screen holds a small program that returns one colour per LED.
It runs where the screen or the music would, so brightness, white balance, the
current limit and smoothing all apply to whatever it draws.

```rust
fn render(ctx) {
    let out = [];
    for i in 0..ctx.n {
        let h = i.to_float() / ctx.n.to_float() + ctx.t * 0.08;
        out.push(hsv(h, 1.0, 1.0));
    }
    out
}
```

The language is [Rhai](https://rhai.rs). One function is required, `render`,
and it returns an array with one entry per LED.

## Colours

Colours are `[r, g, b]` in **linear light**, `0.0` to `1.0`. Not sRGB codes:
everything from the reducer to the final encode is linear, and a script sits in
the middle of that.

A bare number is read as grey, so `[0.5, 1.0]` is a mid grey followed by white.

Values outside `0.0` to `1.0` are clamped. An array shorter than the chain
leaves the rest dark rather than repeating, because repeating would hide the
mistake.

## What `ctx` carries

| Field | Meaning |
|---|---|
| `ctx.n` | LEDs in the chain |
| `ctx.t` | Seconds since the effect started |
| `ctx.dt` | Seconds since the last frame |
| `ctx.energy` | Audio loudness, `0.0` to `1.0` |
| `ctx.beat` | `true` on the frame a beat was detected |
| `ctx.pulse` | Fades from `1.0` after each beat |
| `ctx.bands` | Audio levels per band |

The audio fields are zero unless the audio mode is running or blended in. A
script that reacts to music needs **Mix into the screen** above zero, or the
profile in audio mode, so the analyser is actually started.

## Helpers

| Function | What it does |
|---|---|
| `hsv(h, s, v)` | Colour from hue, saturation and value, all `0.0` to `1.0` |
| `rgb(r, g, b)` | Colour from three channels |
| `mix(a, b, t)` | Blend between two numbers |
| `wave(t)` | A sine that runs from `0.0` to `1.0` |
| `clamp01(v)` | Hold a number inside `0.0` to `1.0` |

Plus Rhai's arithmetic, comparison, array and iterator functions.

## The sandbox

A script cannot reach the file system, the network, or any process. The engine
is built raw, with a hand picked set of packages, so everything reachable is on
the list above and nothing else. `eval` is disabled: a script that can build
code at runtime cannot be reviewed by reading it, and scripts are meant to be
shared as text.

One `render` call is capped at a hundred thousand operations. An accidental
infinite loop therefore costs a single dark frame, not the lights: the call is
abandoned, the error appears under the editor, and the next frame tries again.
Array size, string size, recursion depth and expression depth are bounded too.

This is enforced by tests, including one that runs `loop { x += 1; }` and
asserts that it stops quickly, leaves the chain dark and reports the failure.

## Examples

Four are built in, loadable from the Developer screen.

**Rainbow.** A hue that walks along the strip and drifts with time.

**Breathing.** One colour rising and falling once every four seconds:

```rust
fn render(ctx) {
    let v = wave(ctx.t * 0.25) * 0.9 + 0.1;
    let out = [];
    for i in 0..ctx.n { out.push(hsv(0.08, 0.85, v)); }
    out
}
```

**Beat pulse.** Dark until the music hits, then a flash spreading from the
middle:

```rust
fn render(ctx) {
    let out = [];
    for i in 0..ctx.n {
        let along = i.to_float() / ctx.n.to_float();
        let v = clamp01(ctx.pulse - (along - 0.5).abs());
        out.push(hsv(0.55 + ctx.energy * 0.2, 1.0, v));
    }
    out
}
```

**Meter.** The strip as a level meter, green at the bottom through red at the
top.

## Driving it from outside

A script covers a look that changes on its own. For a look that changes because
something else happened, the [local API](api.md) is the better tool: post a
colour when a build fails, switch a profile when a light goes on.

The two combine: hold a script in the Effect profile, then have a rule or an
API call switch to that profile.
