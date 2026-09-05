---
title: MasLight
---

# MasLight

Screen capture ambilight for Windows, Linux and macOS. Open source, MIT
licensed, and entirely offline: the only network traffic it makes is the
packets it sends to your LED controller.

## Pages

* [Installing](install.md)
* [Linux: X11 and Wayland](linux.md)
* [The audio mode](audio.md)
* [Output protocols](protocols.md)
* [The layout model](layout.md)
* [Building from source](building.md)
* [How it works](architecture.md)

## What it drives

Addressable strips through a controller: WLED over UDP, DDP, sACN (E1.31),
Art-Net, or a directly attached Arduino or ESP speaking Adalight or TPM2.
WS2812B, SK6812 (including RGBW), WS2815 and APA102 are all covered by the
colour order and RGBW settings.

## What makes it different

**It reads the screen in linear light.** A half black, half white screen
averages to a mid grey that is genuinely half as bright, not the far too bright
value you get by averaging sRGB codes. Every stage of the pipeline stays in
linear light until the final encode.

**It reads back a small image, not a whole desktop.** On Windows the desktop is
copied into a texture with a full mip chain, the GPU builds the pyramid, and
MasLight reads back a 240x135 mip. That is 64 times less data over the bus than
a full frame, and the box filter that produced the mip is exactly the average
the LEDs want.

**It slows down when your screen does not change.** A still desktop drops the
capture rate to the idle setting and climbs straight back the moment anything
moves.

**Its output is tested byte for byte.** Every protocol has unit tests that
assert on the exact bytes on the wire, because a controller will happily accept
a malformed packet and show you nothing.
