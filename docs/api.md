---
title: Local API
---

# The local API

A REST and WebSocket surface for scripts, Stream Deck buttons, Home Assistant,
and anything else that wants to drive the lights without the interface.

Three rules shape it:

* **Loopback only.** It binds to `127.0.0.1`. Nothing on your network can
  reach it.
* **Token required.** A machine with more than one program on it is a machine
  where one program should not be able to drive another one's lights.
* **Off by default.** An always-listening socket is not something an ambilight
  opens without being asked.

Turn it on in **Settings**, where the address, the token and a ready made
`curl` line are shown. The token is stored in your configuration file and
survives restarts; **New token** issues a fresh one and revokes every script
that had the old one.

## Authentication

Send the token as a bearer header:

```sh
curl -H "Authorization: Bearer YOUR_TOKEN" http://127.0.0.1:4599/api/status
```

A browser WebSocket cannot set headers, so `?token=YOUR_TOKEN` is accepted too.

## Endpoints

### `GET /api/health`

The only endpoint that needs no token, because a script's first question is
whether MasLight is running at all.

```json
{ "name": "maslight", "version": "0.1.0" }
```

### `GET /api/status`

Live telemetry, the same structure the interface polls: frame rate, frame
time, the colours currently on the strip, device health, capture backend,
source resolution, detected letterbox insets, audio level, and what the rule
sampler last saw.

```json
{
  "running": true,
  "enabled": true,
  "profile": "Desk",
  "profileId": "desk",
  "fps": 60,
  "frameMs": 2.4,
  "ledCount": 64,
  "leds": [{ "r": 12, "g": 30, "b": 90 }],
  "captureBackend": "Dxgi",
  "sourceWidth": 1920,
  "sourceHeight": 1080,
  "audioActive": false,
  "audioEnergy": 0.0
}
```

### `GET /api/config` and `PUT /api/config`

The whole configuration as JSON, exactly as it sits on disk. A `PUT` validates,
clamps, persists and applies it, and answers with what was actually stored, so
you can see what your values were normalised to.

This is the endpoint for anything the other endpoints do not cover: colours,
layout, devices, rules.

### `POST /api/enabled`

```sh
curl -X POST -H "Authorization: Bearer $T" -H "Content-Type: application/json" \
  -d '{"enabled":false}' http://127.0.0.1:4599/api/enabled
```

Answers with what was committed, not with the engine status. The engine applies
a change on its next frame, so a status read taken immediately after a command
would still describe the state from before it.

```json
{ "enabled": false, "activeProfile": "desk" }
```

### `POST /api/profile`

```sh
-d '{"id":"cinema"}'
```

`404` when there is no profile with that id.

### `POST /api/hold`

Holds every LED at one colour, ignoring capture. `null` releases it.

```sh
-d '{"color":"#ff8800"}'
-d '{"color":null}'
```

Answers `204`, or `400` if the colour is not `#rrggbb`.

### `POST /api/identify`

Flashes a single LED white, which is how the calibration wizard finds where
each one physically sits.

```sh
-d '{"index":12,"ms":1500}'
```

### `GET /api/stream`

A WebSocket that pushes the status object ten times a second. Use it for a
dashboard or a live preview rather than polling `/api/status`.

```js
const ws = new WebSocket(`ws://127.0.0.1:4599/api/stream?token=${token}`);
ws.onmessage = (e) => {
  const status = JSON.parse(e.data);
  console.log(status.fps, status.leds.length);
};
```

## Recipes

**A Stream Deck button that toggles the lights.** Read `/api/status`, flip
`enabled`, post it back.

**Warm the lights when the desk lamp goes on.** From Home Assistant, post a
profile change on the automation that switches the lamp.

**A colour on a build failure.** Hold red, then release it when the build goes
green:

```sh
T=your-token
curl -sX POST -H "Authorization: Bearer $T" -H "Content-Type: application/json" \
     -d '{"color":"#ff0000"}' http://127.0.0.1:4599/api/hold
# later
curl -sX POST -H "Authorization: Bearer $T" -H "Content-Type: application/json" \
     -d '{"color":null}' http://127.0.0.1:4599/api/hold
```

## What it is not

There is no discovery, no pairing and no remote access. If you want to reach
MasLight from another machine, put your own reverse proxy in front of it and
take responsibility for that decision; the application will not open the door
for you.
