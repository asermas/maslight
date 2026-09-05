---
title: Camera discovery
---

# Finding the LEDs with a camera

Every ambilight tool asks you to describe your strip. How many on the top,
which corner it starts in, which way it runs, whether you skipped one at the
bottom. That is a translation job, and people get it wrong, because the answer
depends on where you were standing when you wound the strip on.

MasLight can ask the strip instead.

## What happens

It lights a short sequence of patterns. You photograph each one from the same
spot. Both the position of every LED and the order they are wired in fall out
of the photographs.

**The sequence is logarithmic.** With `n` LEDs it takes `ceil(log2(n + 1))`
patterns plus two brackets: sixty LEDs is eight photographs, not sixty. In
pattern `k`, every LED whose index plus one has bit `k` set is lit, so reading
the bits back for one spot spells out its number.

Index *plus one*, because an LED that is dark in every pattern would otherwise
be indistinguishable from one the camera never saw.

The two brackets are everything off and everything on. Their difference is
where the LEDs are; every other frame only has to answer "is this one lit".

## Doing it

1. **Generate a layout first** so MasLight knows how many LEDs there are. The
   positions are what discovery replaces, not the count.
2. Put your phone somewhere steady where it can see the whole screen and the
   strip. Do not move it once you start.
3. Turn the room lights off. The darker the room, the cleaner the difference.
4. Photograph each pattern in order, pressing Next between them.
5. Load all the photographs at once. They are read in filename order, which is
   the order a phone numbers them.
6. Click the four corners of the screen: top left, top right, bottom right,
   bottom left.

The corners are what turns positions in a photograph into positions on the
screen. A monitor photographed from an armchair is a general quadrilateral, so
MasLight solves the homography that takes those four points back to a unit
square and puts every LED through it. That is the difference between positions
that are roughly right and positions that are right.

## What you get

A layout where every LED samples the part of the screen it is actually next to,
in the order it is actually wired. LEDs outside the screen, which is all of
them on a normal build, are pulled to the nearest edge, which is where an LED
should sample from anyway.

**An LED the camera could not see stays in the chain, dark.** Behind a monitor
stand is the usual reason. Removing it would shift every LED after it by one,
which is the worst outcome available, so MasLight keeps the slot and disables
it. You can enable it and place it by hand in Layout Studio.

## When it struggles

**A bright room.** The detector looks for what changed between the dark frame
and the lit frame. A lamp that was on in both is invisible to it, but a room
bright enough to wash out the LEDs is not.

**A reflection in the screen.** A strip reflected in a glossy panel is a second
set of lit spots. They usually get numbered as duplicates of the real ones and
the later reading wins, which puts one LED in the wrong place. Angle the camera
so the reflection is out of frame.

**Moving the camera.** Every frame has to be from the same viewpoint, because
the position of a spot is read from the lit frame and its state is read from
each pattern frame.

## Verifying

The whole pipeline is tested against rendered photographs with known answers: a
screen quad seen at an angle, forty LEDs placed around it, camera noise, and
some LEDs hidden. The tests assert that the chain order comes back exactly,
that each quarter of the strip lands on the edge it was photographed on, that
the bottom edge runs the way it was wound, and that a hidden LED is reported
rather than invented.
