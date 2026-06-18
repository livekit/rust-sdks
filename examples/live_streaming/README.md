# live_streaming

A live-streaming **load test**: one publisher publishes a **simulcast H.264**
video track (a generated SMPTE color-bar test pattern — no camera needed), then
a configurable number of subscribers (40 by default) join the same room at
**random moments spread across a short window** (4 seconds by default), like an
audience flooding into a live stream.

The program logs, per subscriber, when it connects, subscribes to the video
track, and receives its first decoded frame, then prints a final tally.

## Connection

Provide credentials via flags or environment variables:

- `--url` / `LIVEKIT_URL`
- `--api-key` / `LIVEKIT_API_KEY`
- `--api-secret` / `LIVEKIT_API_SECRET`

## Usage

The publisher and subscribers can run as **separate processes** (`publisher` /
`subscriber` subcommands) or **together** (`all`). Connection flags are global
and may appear before or after the subcommand.

### Separate processes

```bash
# Terminal 1 — publisher (stays up until Ctrl-C)
cargo run --release -p live_streaming -- \
  --url wss://your.livekit.server --api-key KEY --api-secret SECRET \
  --room-name my-stream \
  publisher --width 1280 --height 720 --fps 30

# Terminal 2 — 40 subscribers joining over 4s, held for 30s
cargo run --release -p live_streaming -- \
  --url wss://your.livekit.server --api-key KEY --api-secret SECRET \
  --room-name my-stream \
  subscriber --subscribers 40 --join-window 4 --hold 30
```

You can run several subscriber processes against the same room; give each a
distinct `--identity-prefix` (default `sub`) so identities don't collide.

### Combined (one process)

```bash
cargo run --release -p live_streaming -- \
  --url wss://your.livekit.server --api-key KEY --api-secret SECRET \
  --room-name my-stream \
  all --subscribers 40 --join-window 4 --hold 30
```

## Flags

Global (any subcommand): `--url`, `--api-key`, `--api-secret`, `--room-name`
(default `live-streaming-loadtest`). Connection flags fall back to
`LIVEKIT_URL` / `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET`.

`publisher`:
- `--identity <name>`: publisher identity (default `publisher`).
- `--width` / `--height` / `--fps`: publish resolution and framerate (default `1280x720@30`).

`subscriber`:
- `--subscribers <n>`: number of subscribers to spawn (default `40`).
- `--join-window <secs>`: subscribers join at random times within this window (default `4`).
- `--hold <secs>`: keep subscribers connected this long after everyone joined (default `15`).
- `--identity-prefix <prefix>`: identities are `<prefix>-<index>` (default `sub`).

`all` accepts all of the `publisher` and `subscriber` flags.

The publisher always uses H.264 with simulcast enabled (layers derived from the
SDK default presets for the resolution), and the color bars scroll horizontally.
Each subscriber requests the highest simulcast layer as soon as it subscribes.

> Tip: run with `--release` so the H.264 encoder and many concurrent decoders keep up.
