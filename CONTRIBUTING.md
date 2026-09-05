# Contributing to MasLight

Patches are welcome. This file is short on purpose.

## Getting set up

See [building from source](docs/building.md). The interface alone needs only
Node:

```sh
npm install --prefix app
npm run dev --prefix app
```

That serves the whole interface against a demo backend, so you can work on any
screen without a controller, a strip, or even Rust.

## Before you open a pull request

```sh
cargo fmt --all
cargo clippy --workspace --exclude maslight-app --all-targets -- -D warnings
cargo test --workspace --exclude maslight-app
npm run build --prefix app
```

Continuous integration runs exactly these, plus a Linux job that compiles the
Wayland backend and a job that regenerates the icons and fails if they differ
from what is committed.

## House rules

**A new output protocol needs a byte level test.** A controller will accept a
malformed packet and simply show nothing, so an integration test that "the
lights came on" proves very little. Look at `crates/maslight-output/tests/packets.rs`
and assert the header fields by offset.

**Colour work stays in linear light.** Everything between the reducer and the
final encode is linear. If you find yourself converting to sRGB in the middle
of the pipeline, something has gone wrong.

**Platform code lives behind the trait.** `CaptureBackend` and `Sink` are the
seams. Nothing outside `maslight-capture` should know what DXGI is.

**Comments explain why, not what.** The code already says what it does.

**Be honest in the interface.** A screen for a feature that is not finished
says so. It does not show a control that quietly does nothing.

## Things that would help most

* **Running the X11 backend on a real Linux desktop.** It is written and it
  compiles, but nobody has watched it drive a strip yet.
* **Finishing the PipeWire reader.** The portal half is done and tested in CI;
  see `crates/maslight-capture/src/pipewire.rs` and
  [the Linux page](docs/linux.md).
* **A macOS ScreenCaptureKit backend.** The trait is small and the synthetic
  source in `test_source.rs` shows the shape.
* **Testing against controllers other than WLED.** DDP, sACN and Art-Net are
  correct against the specifications but have not met real hardware.

## Reporting a problem

Include your operating system and session type (X11 or Wayland), your
controller and protocol, and the output of:

```sh
MASLIGHT_LOG=debug maslight
cargo run -p maslight-capture --example probe
```

The probe prints what the capture backend and reducer actually see, which
answers most questions immediately.

## Licence

By contributing you agree that your work is published under the MIT licence,
the same as the rest of the project.
