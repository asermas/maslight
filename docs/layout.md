---
title: Layout
---

# The layout model

A layout says, for every LED in the physical chain, which display it looks at
and which rectangle of that display it samples.

Coordinates are **normalised per display**, `0.0` to `1.0`, origin top left. A
resolution change therefore never invalidates a layout, and the same layout
file works on a 1080p laptop and a 4K monitor.

## The file

```json
{
  "version": 1,
  "name": "Desk",
  "displays": [
    { "id": "\\\\.\\DISPLAY1", "label": "Display 1", "bounds": [0, 0, 2560, 1440] }
  ],
  "chain": { "colorOrder": "grb", "rgbw": false, "reverse": false },
  "leds": [
    {
      "index": 0,
      "display": "\\\\.\\DISPLAY1",
      "rect": { "x": 0.0, "y": 0.88, "w": 0.05, "h": 0.12 },
      "weight": 1.0,
      "enabled": true
    }
  ]
}
```

### Fields

* `index` is the position in the physical chain and is always contiguous. An
  LED that is missing or intentionally dark stays in the list with
  `enabled: false`, so the wire order is never disturbed.
* `display` names the display this LED samples. An **empty string** means
  effect only: the LED is driven by effects and ignores the screen.
* `rect` is the sampling rectangle. `w` and `h` control how much of the screen
  the LED averages; a deeper rectangle is calmer, a shallower one is more
  responsive to edges.
* `weight` scales the LED when averaging, mostly used to soften corners.

### Display identifiers

* Windows: `\\.\DISPLAY1`, from Desktop Duplication.
* X11: the RandR output name, such as `eDP-1` or `HDMI-1`.
* Wayland: `portal`, because the portal decides which output is shared and does
  not tell the application about the others.

If a layout names a display that is no longer present, MasLight falls back to
the primary one instead of going dark, so unplugging a monitor does not break a
profile.

## The wizard

The generator takes an LED count for each edge, a starting corner, a direction,
and a depth, and produces the chain in wire order. Then you edit individual
LEDs on the canvas.

* **Starting corner and direction** describe where the first LED physically
  sits and which way the strip runs when you look at the screen.
* **Chain offset** rotates the finished chain. Use it when the strip starts
  halfway along an edge rather than in a corner, which is common when the
  controller sits behind the middle of a monitor.
* **Reverse chain** flips the order, which saves rewiring an upside down strip.
* **Edge coverage** is for strips that do not reach all the way into the
  corners.

## Sampling cost

The reducer takes at most `sampleGrid` squared taps per LED, so the cost is set
by the LED count and not by the resolution. Sixty LEDs at the default grid of
16 is 15 360 taps per frame, roughly thirty microseconds on one core. A 4K
frame and a 720p frame cost exactly the same.

## Letterbox detection

When enabled, MasLight walks in from each edge looking for black bars and pulls
every sampling rectangle into the picture area. Without it, the bottom row of
LEDs shows nothing but the black bar during a film. The result is quantised so
one noisy pixel row cannot make the bars twitch, and it never eats more than
40 percent of a side.
