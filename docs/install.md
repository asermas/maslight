---
title: Installing
---

# Installing

## Packages

Every tagged release publishes packages built by continuous integration:

| System | File |
|---|---|
| Windows 10 and 11 | `MasLight_x.y.z_x64-setup.exe` or the `.msi` |
| Debian and Ubuntu | `MasLight_x.y.z_amd64.deb` |
| Fedora | `MasLight-x.y.z-1.x86_64.rpm` |
| Any Linux | `MasLight_x.y.z_amd64.AppImage` |

These are the names the bundler actually produces, not names anybody wrote
down and hoped for: the packaging workflow builds all five on every change and
keeps them, so the table can be checked against real files.

Installing the Debian package pulls in what it needs:

```bash
sudo apt install ./MasLight_0.1.0_amd64.deb
```

The package is named `mas-light` inside the archive, which is what the bundler
makes of the name MasLight. `apt remove mas-light` is therefore the way to
take it off again.

macOS packages arrive with the 0.3 milestone, when the ScreenCaptureKit
backend lands.

## First run

1. **Devices.** Press *Scan the network*. WLED controllers announce themselves
   over mDNS and MasLight then asks each one how many LEDs it drives. If your
   network blocks multicast, type the address by hand; that always works.
2. **Layout.** Enter how many LEDs sit along each edge, pick the corner the
   first LED is in and which way the strip runs, then press *Generate layout*.
   Drag individual LEDs on the canvas afterwards if the strip is not evenly
   spaced.
3. **Calibration.** Three steps: colour order, white balance, and output delay.
   The first one alone fixes the most common complaint, which is a strip that
   shows green when the screen is red.
4. **Studio.** Brightness, smoothing, eye care and the current limit.

## When something is wrong

MasLight writes a log next to its configuration, rewritten on each launch:

| System | Path |
|---|---|
| Windows | `%APPDATA%\MasLight\maslight.log` |
| macOS | `~/Library/Application Support/MasLight/maslight.log` |
| Linux | `~/.config/maslight/maslight.log` |

It says which configuration file was read and how many LEDs and devices came
out of it, which capture backend started and at what size, and anything that
failed. An application started from a shortcut or at login has no console, so
without the file there is nothing to look at. A healthy start looks like this:

```text
INFO maslight_app: configuration read from ...\config.json (20688 bytes on disk, 1 profile(s))
INFO maslight_app: profile "Desk" (desk), mode Screen, 64 leds, 1 device(s), enabled true
INFO maslight_capture::dxgi: DXGI capture on \\.\DISPLAY1 at 1920x1080, reading back 240x135 from mip 3
```

If the profile line says `"Default" (default), 0 leds`, MasLight is not reading
the configuration you think it is: the path on the line above is the one it
actually used.

Set `MASLIGHT_LOG=debug` for more.

## Where the configuration lives

| System | Path |
|---|---|
| Windows | `%APPDATA%\MasLight\config.json` |
| macOS | `~/Library/Application Support/MasLight/config.json` |
| Linux | `~/.config/maslight/config.json` |

It is plain JSON, written atomically, and safe to edit or copy between
machines. Settings has a button that opens the folder.

## Power

A WS2812B pixel draws about 20 mA per channel, so 60 mA at full white. Sixty
LEDs is therefore 3.6 A in the worst case, which is more than a USB port will
give you.

Two things help:

* The **current limit** in Studio scales the whole strip down rather than
  letting it exceed the budget you set.
* Power injection at both ends of a long strip, so the far end does not fade to
  brown.

The estimated draw at full white is shown next to the limit.

## Privacy

MasLight has no account, no analytics and no crash reporting. Update checking
is off by default, and turning it on only fetches the release list from GitHub.
The only other traffic it makes is the packets it sends to your controller on
your own network.
