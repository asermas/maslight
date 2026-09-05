---
title: Building
---

# Building from source

## What you need

* Rust, stable. Install with [rustup](https://rustup.rs).
* Node 20 or newer, for the interface.
* Platform toolchain, below.

## Windows

The normal route is the MSVC toolchain:

```powershell
rustup default stable-x86_64-pc-windows-msvc
```

That needs the Visual Studio C++ build tools and the Windows SDK, which the
Visual Studio Installer provides under **Desktop development with C++**.

If you would rather not install several gigabytes of Visual Studio, MasLight
also builds with the GNU toolchain and a portable MinGW:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup default stable-x86_64-pc-windows-gnu
```

Then put a MinGW `bin` directory on PATH. [w64devkit](https://github.com/skeeto/w64devkit)
works and needs no administrator rights. Two details:

* `dlltool`, `as` and `windres` must be on PATH. `windres` shells out to `gcc`,
  so the whole `bin` directory needs to be there, not just the three files.
* The Rust GNU toolchain asks the linker for `-lgcc_eh`, which w64devkit does
  not ship because its `libgcc.a` already carries the unwinder. Create an empty
  archive so the linker is satisfied:

  ```sh
  cd <w64devkit>/lib/gcc/x86_64-w64-mingw32/<version>
  ar rcs libgcc_eh.a
  ```

## Linux

```sh
sudo apt install libx11-dev libxcb1-dev libxcb-shm0-dev libxcb-randr0-dev \
                 libdbus-1-dev libasound2-dev pkg-config build-essential
# for the desktop app
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev \
                 libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev libasound2-dev
```

## macOS

The toolchain that comes with Xcode command line tools is enough:

```sh
xcode-select --install
```

The ScreenCaptureKit backend is not written yet, so the app builds but has no
capture source on macOS. That is the 0.3 milestone.

## Build and run

```sh
# libraries and tests, no interface needed
cargo test --workspace --exclude maslight-app

# the interface on its own, in a browser, with a demo backend
npm install --prefix app
npm run dev --prefix app

# the whole desktop app
npm run build --prefix app
cargo run -p maslight-app
```

## Optional features

| Feature | Crate | What it adds |
|---|---|---|
| `serial` | `maslight-app`, `maslight-output` | Adalight and TPM2 over USB. Needs libudev on Linux. |
| `portal` | `maslight-capture` | The xdg-desktop-portal handshake. |
| `wayland` | `maslight-capture` | The PipeWire stream reader. Needs `libpipewire-0.3-dev`. |
| `discovery` | `maslight-output` | mDNS discovery of WLED controllers. On by default. |

## Icons

Every icon is generated from the same geometry as `assets/brand/mark.svg`:

```sh
node scripts/make-icons.mjs
```

Continuous integration re-runs this and fails if the result differs, so the SVG
and the rasters cannot drift apart.

## Diagnosing capture

`maslight-capture` ships a probe that lists displays, captures a burst and
prints what the reducer sees:

```sh
cargo run -p maslight-capture --example probe
cargo run -p maslight-capture --example probe test   # synthetic source
```

Logging is controlled by `MASLIGHT_LOG`:

```sh
MASLIGHT_LOG=debug cargo run -p maslight-app
```
