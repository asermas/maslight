---
title: Audio
---

# The audio mode

MasLight listens to what your speakers are playing, not to a microphone. There
is nothing to route and nothing to plug in: it opens the output device in
loopback and hears exactly what you hear.

* **Windows** uses WASAPI loopback, which is what opening an input stream on an
  output device gives you.
* **Linux** picks a PipeWire or PulseAudio monitor source.
* **macOS** has no system loopback of its own, so a virtual device such as
  BlackHole is needed. That is on the list.

## What it measures

One window of 1024 samples, which is 21 ms at 48 kHz. That is short enough to
feel immediate and long enough to resolve down to about 47 Hz.

**Bands are spaced logarithmically** between 40 Hz and 16 kHz. Music has most of
its energy in the bottom two octaves, so linear bands give you a strip where the
first two LEDs move and the rest sit still.

**Loudness is measured across the whole window**, not as an average of the
bands. A pure tone fills one band in sixteen; averaging bands would call a loud
sine quiet, which is not what anyone hears.

**Beats are positive spectral flux in the bottom quarter** of the spectrum
against an adaptive threshold, with a refractory period so one kick is one beat.
The `beatSensitivity` setting is how many standard deviations above the running
mean counts.

## Effects

| Effect | What it does |
|---|---|
| **Spectrum** | The spectrum mirrored around the middle of the strip, so bass lands in the corners nearest the desk and treble in the middle. |
| **Energy** | The whole strip on one hue, brightness following loudness. |
| **Wave** | A front travelling outwards from the middle on each beat. |
| **Scroll** | A hue that drifts continuously, brightness following the music. |

## Mixing with the screen

Audio does not have to take the whole strip. In screen mode, **Mix into the
screen** blends the two in linear light before the colour pipeline, so
brightness, white balance and the current limit apply once to the result rather
than twice to the parts.

Set it to zero and audio capture does not start at all. There is no cost to
having the mode available.

## Tuning

The two thresholds are where most of the character lives:

* **Quiet threshold** is the level that maps to nothing. Raise it if the strip
  never goes dark between tracks.
* **Loud threshold** is the level that maps to full. Lower it if the strip
  never reaches full brightness on your normal listening volume.

Most streaming music is mastered around -14 LUFS, which the defaults are
centred on. Classical recordings sit far lower and usually want the quiet
threshold pulled down.

**Attack and release** are the two speeds. A fast attack and a slow release is
what makes a spectrum look alive rather than jittery. Equal values look
mechanical.

## Cost

The capture callback does nothing but copy samples into a ring buffer, so the
audio device is never waiting on anything. Analysis is one 1024 point FFT per
frame, which is tens of microseconds, and everything after it is per band.
