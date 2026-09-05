---
title: Linux
---

# Linux: X11 and Wayland

This is the page that explains why MasLight exists. Prismatik, the tool most
people used for this, is effectively Windows only today: its Linux build binds
to X11 and does nothing at all in a Wayland session, which is what Ubuntu,
Fedora and most current desktops boot into by default.

## X11 sessions

Fully supported. The backend is exercised for real in continuous integration:
a job under Xvfb paints the root window red, captures it, and asserts the
reducer sees red rather than some other channel. Nobody has yet watched it
drive a physical strip on a physical desktop, which is the one gap left.

MasLight uses the MIT-SHM extension: the X server writes each frame into a
shared memory segment instead of pushing it down the socket, and MasLight then
samples only the pixels its LEDs need. If the extension is missing, which
happens on remote displays, it falls back to plain `GetImage` and logs a
warning.

Multiple monitors are enumerated through RandR, so each LED can be bound to a
specific output.

### Getting an X11 session on Ubuntu

At the login screen, click the gear icon in the bottom right after selecting
your user, and choose **Ubuntu on Xorg**. The choice is remembered.

## Wayland sessions

Wayland has no root window to grab. Everything goes through
`xdg-desktop-portal`: the compositor shows a dialog asking which screen you
want to share, and hands back a PipeWire stream.

MasLight implements this in two halves:

* **The portal handshake** is complete and compiled in continuous integration.
  It asks for a **restore token** and stores it in `portal-token` next to your
  configuration, so the consent dialog appears once rather than on every start.
  Delete that file if you want to be asked again.
* **The PipeWire stream reader** is written but has not yet run on real
  hardware. It lives behind the `wayland` cargo feature and is compiled, but not
  executed, by continuous integration.

Until that second half is verified, the honest recommendation on a Wayland
desktop is to log in to an X11 session. This page will be updated when the
PipeWire path has been used in anger rather than merely compiled.

To build with it anyway:

```sh
sudo apt install libpipewire-0.3-dev libspa-0.2-dev pkg-config clang
cargo build -p maslight-capture --features wayland
```

## Dependencies

For a normal build:

```sh
sudo apt install libx11-dev libxcb1-dev libxcb-shm0-dev libxcb-randr0-dev \
                 libdbus-1-dev pkg-config
```

The desktop app additionally needs the WebKit runtime that Tauri uses:

```sh
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev \
                 libayatana-appindicator3-dev librsvg2-dev libxdo-dev
```

## Starting with the session

The **Start with the system** switch in Settings writes
`~/.config/autostart/maslight.desktop`. It is a plain text file you can read
and delete without the app.

## Serial output

Serial output is behind the `serial` cargo feature because it pulls in libudev.
If you drive an Arduino over USB rather than a controller over the network:

```sh
sudo apt install libudev-dev
cargo build -p maslight-app --features serial
```

You will also need permission on the port, usually by adding yourself to the
`dialout` group.
