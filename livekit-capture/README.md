# livekit-capture

Capture helpers for publishing decoded, native platform, DMA-BUF, and
pre-encoded video frames with the LiveKit Rust SDK.

Optional source features include `avfoundation`, `libargus`, `v4l`,
`tcpsink`, `rtsp`, and `gstreamer`.

## Pre-encoded source modes

The `preencode_publish` example can publish H.264, H.265, VP8, VP9, and AV1
access units from these sources:

| Source | Feature | Input shape |
| --- | --- | --- |
| `gstappsink` | `gstreamer` | Generated or custom GStreamer pipeline ending in `appsink` or one unlinked encoded pad |
| `tcpsink` | `tcpsink` | TCP connection to an encoded byte-stream or RFC4571 RTP producer |
| `shmsink` | `gstreamer` | GStreamer `shmsink` producer read through `shmsrc` |
| `rtsp` | `rtsp` | RTSP over TCP with interleaved RTP video |

H.264/H.265 TCP defaults remain Annex-B byte streams. VP8, VP9, and AV1 use RTP
framing over TCP because those codecs need explicit frame boundaries.

## Pre-encoded test sources

The `preencode_publish` example includes GStreamer fixture scripts for testing
the H.264, H.265, VP8, VP9, and AV1 pre-encoded capture paths with an animated
`videotestsrc` source at `1280x720@30fps`.
The generated encoder pipelines force 8-bit I420 input; VP9 fixture caps are
pinned to profile 0 to match the WebRTC passthrough profile.

Before running a publisher command, provide LiveKit credentials through the
environment or command-line flags:

```sh
export LIVEKIT_URL=wss://example.livekit.cloud
export LIVEKIT_API_KEY=devkey
export LIVEKIT_API_SECRET=secret
```

All scripts require `--codec h264|h265|vp8|vp9|av1`. They also accept `--width`,
`--height`, `--fps`, `--bitrate-kbps`, and `--print`; the defaults match the
test profile above.

### Runtime status

The unit and fixture coverage exercises H.264, H.265, VP8, VP9, and AV1 ingest
through GStreamer appsink, TCP RTP, shared-memory shmsink, and RTSP RTP.
H.264/H.265 TCP byte-stream ingest remains the compatibility default.

Local-SFU smoke testing has verified subscriber decode for H.264, H.265, VP8,
VP9, and AV1 through GStreamer appsink, TCP RTP, shared-memory shmsink, and
RTSP RTP sources. The generated fixture uses a low-motion animated test pattern
so the encoded source stays near the advertised publish cap; high-entropy custom
pipelines may need an explicit `--max-bitrate` large enough for the frames they
produce.

### Local SFU smoke

With a local LiveKit server running in dev mode:

```sh
livekit-server --dev --bind 127.0.0.1
```

Use the dev credentials in the publisher examples:

```sh
export LIVEKIT_URL=ws://127.0.0.1:7880
export LIVEKIT_API_KEY=devkey
export LIVEKIT_API_SECRET=secret
```

Run a subscriber in another terminal to verify negotiated codec and decoder
health:

```sh
cargo run -p local_video --features desktop --bin subscriber -- \
  --url "$LIVEKIT_URL" \
  --api-key "$LIVEKIT_API_KEY" \
  --api-secret "$LIVEKIT_API_SECRET" \
  --room-name video-room \
  --identity sub-vp8 \
  --participant gst-vp8-pub \
  --display-timestamp
```

Then publish a pre-encoded GStreamer fixture:

```sh
cargo run -p preencode_publish --features gstreamer -- \
  --source gstappsink \
  --codec vp8 \
  --url "$LIVEKIT_URL" \
  --api-key "$LIVEKIT_API_KEY" \
  --api-secret "$LIVEKIT_API_SECRET" \
  --room-name video-room \
  --identity gst-vp8-pub \
  --width 1280 \
  --height 720 \
  --fps 30 \
  --diagnostics
```

Expected publisher signs are a successful room connection, a
`Published pre-encoded ... track at 1280x720` log line, and diagnostics near
30 access units per second. Expected subscriber signs for healthy codecs are a
matching `Subscribed to video track` codec and rising decoded-frame counts with
low loss and no repeated PLI loop.

### GStreamer `gstappsink` Source

This exercises:

`GStreamer videotestsrc -> encoder -> appsink -> GStreamerAppSinkEncodedSource -> VideoCaptureTrack`

Publish the generated GStreamer source:

```sh
cargo run -p preencode_publish --features gstreamer -- \
  --source gstappsink \
  --codec h264 \
  --width 1280 \
  --height 720 \
  --fps 30 \
  --room-name video-room \
  --identity gst-h264-pub \
  --diagnostics
```

For H.265, VP8, VP9, or AV1, change `--codec` to `h265`, `vp8`, `vp9`, or
`av1`. The generated AV1 path inserts `av1parse` and requests
`stream-format=obu-stream,alignment=tu` before appsink.

Custom GStreamer launch fragments can be passed after `--`. If the pipeline
does not include `appsink name=lk_appsink`, it must leave exactly one encoded
video source pad unlinked so the example can attach codec-specific parsing,
caps, and appsink:

```sh
cargo run -p preencode_publish --features gstreamer -- \
  --source gstappsink \
  --codec h264 \
  --width 1280 \
  --height 720 \
  --fps 30 \
  --room-name video-room \
  --identity custom-gst-h264-pub \
  --diagnostics \
  -- \
  'videotestsrc is-live=true do-timestamp=true ! video/x-raw,width=1280,height=720,framerate=30/1 ! videoconvert ! x264enc tune=zerolatency speed-preset=ultrafast key-int-max=30 byte-stream=true aud=true'
```

### TCP `tcpsink` Source

This exercises:

`GStreamer videotestsrc -> encoder -> tcpserversink -> TcpEncodedSource -> VideoCaptureTrack`

The `preencode_publish` CLI source is `tcpsink`; it connects to a TCP producer
such as the fixture script's GStreamer `tcpserversink`.

Start the producer:

```sh
examples/preencode_publish/scripts/run-tcp-test-source.sh --codec h264 --port 5000
```

Publish the TCP source:

```sh
cargo run -p preencode_publish -- \
  --source tcpsink \
  --host 127.0.0.1:5000 \
  --codec h264 \
  --width 1280 \
  --height 720 \
  --fps 30 \
  --room-name video-room \
  --identity tcp-h264-pub \
  --diagnostics
```

For H.265, use `--codec h265` in both commands.

For VP8, VP9, or AV1, use the same script with `--codec vp8`, `--codec vp9`, or
`--codec av1`; `preencode_publish --tcp-format auto` selects RTP automatically:

```sh
cargo run -p preencode_publish -- \
  --source tcpsink \
  --host 127.0.0.1:5000 \
  --codec vp8 \
  --tcp-format auto \
  --width 1280 \
  --height 720 \
  --fps 30 \
  --room-name video-room \
  --identity tcp-vp8-pub \
  --diagnostics
```

### Shared-Memory `shmsink` Source

This exercises:

`GStreamer videotestsrc -> encoder -> shmsink -> shmsrc -> GStreamerAppSinkEncodedSource -> VideoCaptureTrack`

Start the producer:

```sh
examples/preencode_publish/scripts/run-shm-test-source.sh \
  --codec h264 \
  --socket-path /tmp/livekit-preencode-h264.shm
```

Publish by connecting the first-class `shmsink` source to that socket:

```sh
cargo run -p preencode_publish --features gstreamer -- \
  --source shmsink \
  --codec h264 \
  --shmsink-socket-path /tmp/livekit-preencode-h264.shm \
  --width 1280 \
  --height 720 \
  --fps 30 \
  --room-name video-room \
  --identity shm-h264-pub \
  --diagnostics
```

For H.265, use `--codec h265`, a different socket path if desired, and
the same `--source shmsink` command shape.

For VP8/VP9, use `--codec vp8` or `--codec vp9`. For AV1, the producer script
parses to low-overhead temporal units before `shmsink`, and the `shmsink`
source adds the matching AV1 appsink caps:

```sh
cargo run -p preencode_publish --features gstreamer -- \
  --source shmsink \
  --codec av1 \
  --shmsink-socket-path /tmp/livekit-preencode-av1.shm \
  --width 1280 \
  --height 720 \
  --fps 30 \
  --room-name video-room \
  --identity shm-av1-pub \
  --diagnostics
```

### RTSP source

This exercises:

`GStreamer videotestsrc -> encoder -> RTP payloader -> gst-rtsp-server -> RtspEncodedSource -> VideoCaptureTrack`

Start the RTSP server. The script uses the `test-launch` tool from
`gst-rtsp-server` and serves `/test`:

```sh
examples/preencode_publish/scripts/run-rtsp-test-source.sh --codec h264 --port 8555
```

Publish the RTSP source:

```sh
cargo run -p preencode_publish -- \
  --source rtsp \
  --rtsp-url rtsp://127.0.0.1:8555/test \
  --codec h264 \
  --width 1280 \
  --height 720 \
  --fps 30 \
  --room-name video-room \
  --identity rtsp-h264-pub \
  --diagnostics
```

For H.265, use `--codec h265` in both commands.

For VP8, VP9, or AV1, use `--codec vp8`, `--codec vp9`, or `--codec av1` in
both commands. The RTSP fixture switches to `rtpvp8pay`, `rtpvp9pay`, or
`rtpav1pay` automatically.

Publisher-side success signs are a successful room connection, a
`Published pre-encoded ... track at 1280x720` log line, and diagnostics near
30 access units per second.
