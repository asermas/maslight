---
title: Protocols
---

# Output protocols

Every protocol below is implemented as a set of pure packet builders with unit
tests that assert on the exact bytes. If your controller does not light up, the
framing is not the reason.

## WLED UDP realtime

Default port 21324. Byte 0 selects the protocol, byte 1 is how many seconds
WLED waits after the last packet before returning to its own effects.

| id | name   | payload                               | LEDs per packet |
|----|--------|---------------------------------------|-----------------|
| 1  | WARLS  | `index, r, g, b` per LED              | 255 total       |
| 2  | DRGB   | `r, g, b` from LED 0                  | 490             |
| 3  | DRGBW  | `r, g, b, w` from LED 0               | 367             |
| 4  | DNRGB  | 16-bit start index, then `r, g, b`    | 489 per packet  |
| 5  | DNRGBW | 16-bit start index, then `r, g, b, w` | 367 per packet  |

**DNRGB is the default.** It is the only variant that scales past 490 LEDs, and
the two extra bytes cost nothing. MasLight splits a long strip across as many
datagrams as it needs and never emits more than 1472 bytes, which is what fits
in one Ethernet frame after the IP and UDP headers.

Setting the hold time to 255 makes WLED stay in realtime mode indefinitely.
Small values are safer: if MasLight stops, the strip returns to its own effect
rather than freezing on the last frame.

## DDP

Default port 4048. A ten byte header:

```text
0     flags     0x40 version 1, plus 0x01 PUSH on the last packet of a frame
1     sequence  1..15, 0 means unused
2     type      0x0B for RGB with 8 bits per channel, 0x1B for RGBW
3     dest id   1, the default output device
4..7  offset    in bytes, big endian
8..9  length    in bytes, big endian
```

DDP has no universe limit, so a longer strip only means more datagrams. Only
the final packet of a frame carries the PUSH flag, which is what makes the
receiver display the whole frame at once.

## sACN (E1.31)

Port 5568. 512 DMX slots per universe, so 170 RGB LEDs; longer strips spill
into consecutive universes. MasLight sends to the multicast group
`239.255.<universe high>.<universe low>` unless you give it a unicast address.

The packet layout follows ANSI E1.31-2018: a 38 byte root layer, a 77 byte
framing layer and a DMP layer, 126 bytes in total before the first slot. The
tests assert the PDU lengths agree with the packet size, which is the mistake
that makes receivers silently ignore you.

## Art-Net

Port 6454, ArtDmx opcode 0x5000. Also 512 slots per universe. The port address
is assembled from net, subnet and universe as Art-Net 4 specifies, and the slot
count is padded to an even number because the specification requires it.

## OpenRGB

Port 6742. OpenRGB drives motherboard headers, RAM, keyboards, fans and a long
list of other hardware, so speaking its SDK covers all of it with one
integration.

Every packet carries the same sixteen byte header: `ORGB`, the device index,
the packet id, and the payload length, all little endian. MasLight sends two
packets on connect, `SetClientName` so a human can see who is driving their
lights and `SetCustomMode` because a device left in an effect mode ignores
colours, and then `UpdateLeds` per frame.

Colours are four bytes each: red, green, blue, and a padding byte the protocol
reserves. Getting that stride wrong shifts every LED, which looks like a
working connection showing rubbish, so the tests assert it.

## MQTT

Port 1883, and **not a frame stream**. Sixty LEDs sixty times a second is a
thousand messages a second, which is not what a broker is for and not what
anything subscribing actually wants.

MasLight instead publishes a small retained JSON object twice a second:

```json
{
  "state": "ON",
  "leds": 64,
  "color": { "r": 127, "g": 0, "b": 127 },
  "hex": "#7f007f"
}
```

Retained, so a subscriber that connects later learns the current state rather
than waiting for the next tick. A repeat becomes a ping rather than another
publish, so a still screen does not wake anything downstream.

The client is a hand written MQTT 3.1.1 publisher: a CONNECT packet and a
PUBLISH at quality of service zero, which is two packet shapes rather than a
dependency.

## Adalight and TPM2

For a directly attached Arduino or ESP over USB.

* **Adalight** sends `Ada`, the LED count minus one as two bytes, a checksum of
  `high xor low xor 0x55`, then the RGB data. This is what the stock Adalight
  sketch and Prismatik speak, so existing hardware keeps working.
* **TPM2** sends `0xC9 0xDA`, a 16-bit payload length, the data, then `0x36`.

Serial output is behind the `serial` cargo feature.

## Colour order

The chip decides which byte is which colour. WS2812B is `GRB`, WS2811 is
usually `RGB`, APA102 is `BGR`. MasLight applies the order when it builds the
packet, never earlier, so the pipeline and the preview always work in real RGB.

The calibration wizard finds the order for you: it sends pure red and asks what
colour the strip actually shows.
