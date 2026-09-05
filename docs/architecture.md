---
title: How it works
---

# How it works

```text
capture backend  ->  zone reducer  ->  colour pipeline  ->  sink
   (platform)         (per LED)          (per frame)       (bytes)
```

Four crates, each one testable on its own:

| Crate | Contents |
|---|---|
| `maslight-core` | Colour pipeline, layout model, zone reducer, profiles. No platform code at all. |
| `maslight-capture` | `CaptureBackend` trait plus DXGI, X11, PipeWire and a synthetic source. |
| `maslight-output` | `Sink` trait plus WLED, DDP, sACN, Art-Net, serial, and mDNS discovery. |
| `maslight-engine` | The loop, profile handling, telemetry, the delay line. |

The desktop app in `app/` is a shell around `maslight-engine`: a window, a
tray icon, and the commands the interface calls.

## Capture

A backend hands the engine a borrowed `FrameView` rather than an owned buffer,
because the pixels usually live in a mapped GPU staging surface that has to be
released before the next frame. A callback makes that lifetime impossible to
get wrong.

**Windows** uses Desktop Duplication. The naive approach copies the whole
desktop into a staging texture and maps it, which at 2560x1440x60 is 880 MB/s
crossing the bus. MasLight instead copies the desktop into a texture that owns
a full mip chain, asks the GPU to generate the mips, and reads back a small
one. A 1920x1080 desktop comes back as 240x135. The box filter that produced
that mip is exactly the average the reducer wants, so quality goes up while the
readback drops by a factor of 64.

**X11** uses MIT-SHM so the frame never crosses the socket.

**Wayland** uses the xdg-desktop-portal ScreenCast interface and a PipeWire
stream. See [Linux](linux.md) for the current state of that path.

## Reduction

The reducer averages each LED rectangle in linear light through a 256 entry
lookup table. Cost is bounded by the number of taps, not by resolution.

Averaging in linear light is not a detail. A screen that is half black and half
white averages to `0.5` in linear light, which is a mid grey that really is
half as bright. Averaging the sRGB codes instead gives `0.498` encoded, which
is `0.21` in linear light: less than half the brightness the screen is actually
emitting.

## Colour

Nine stages, every one of them optional and serialisable, so a profile fully
describes a look:

1. HDR tone mapping (extended Reinhard, hue preserving)
2. Saturation and vibrance
3. White balance, from a Kelvin value normalised so 6500 K is exactly neutral
4. Brightness
5. Eye care floor or dead zone
6. Temporal smoothing, frame rate independent: `alpha = 1 - e^(-dt/tau)`
7. Power limiting against a current budget
8. RGBW extraction
9. Gamma and temporal dithering

Dithering matters more than it sounds. On a slow fade, 8-bit output bands
visibly; carrying the sub-LSB residual into the next frame turns that banding
into noise below the threshold of perception. The test asserts that a value of
10.5/255 averages to 10.5 over a hundred frames.

## Engine

One worker thread owns capture, colour and output. Everything else sends it
messages and reads a status snapshot, so a slow interface can never stutter the
lights.

Latency compensation is a delay line: frames go into a queue and come out when
they are old enough. That is what lets the strip and the panel change at the
same instant when the panel has its own processing delay.

Adaptive pacing watches whether anything changed. After a second of a still
screen the capture rate drops to the idle setting and returns the moment
something moves.

## Testing

* Colour pipeline: fifteen tests including frame rate independence, dithering
  convergence, power limiting and NaN handling.
* Reduction: nine tests including linear light averaging, padded strides and
  letterbox detection.
* Layout: ten tests including chain ordering for every corner and direction.
* Protocols: twenty three tests asserting the exact bytes of every format.
* End to end: seven tests that run the real engine thread and read real UDP
  packets off a loopback socket.
