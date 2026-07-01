# livekit-capture

Capture helpers for publishing decoded, native platform, DMA-BUF, and
pre-encoded video frames with the LiveKit Rust SDK. Optional source features
include `avfoundation`, `libargus`, `v4l`, `tcpsink`, `rtsp`, and `gstreamer`.

## Pre-encoded source modes

The `preencode_publish` example publishes H.264, H.265, VP8, VP9, and AV1
access units from these sources:

| Source | Feature | Input shape |
| --- | --- | --- |
| `gstappsink` | `gstreamer` | Generated or custom GStreamer pipeline ending in `appsink` or one unlinked encoded pad |
| `tcpsink` | `tcpsink` | TCP connection to an encoded byte-stream or RFC4571 RTP producer |
| `shmsink` | `gstreamer` | GStreamer `shmsink` producer read through `shmsrc` |
| `rtsp` | `rtsp` | RTSP over TCP with interleaved RTP video |

H.264/H.265 TCP defaults to Annex-B byte streams, while VP8, VP9, and AV1 use
RTP framing over TCP because those codecs need explicit frame boundaries.

## Pre-encoded test sources

The example ships GStreamer fixture scripts that exercise the H.264, H.265,
VP8, VP9, and AV1 capture paths with an animated `videotestsrc` at
`1280x720@30fps`. Generated encoder pipelines force 8-bit I420 input, and VP9
fixture caps are pinned to profile 0 to match the WebRTC passthrough profile.

Before running a publisher, provide LiveKit credentials through the environment
or command-line flags:

```sh
export LIVEKIT_URL=wss://example.livekit.cloud
export LIVEKIT_API_KEY=devkey
export LIVEKIT_API_SECRET=secret
```

All scripts require `--codec h264|h265|vp8|vp9|av1` and also accept `--width`,
`--height`, `--fps`, `--bitrate-kbps`, and `--print`; the defaults match the
test profile above.

### Local SFU example

Run a local LiveKit server in dev mode and use its dev credentials in the
publisher examples:

```sh
livekit-server --dev --bind 0.0.0.0
```

```sh
export LIVEKIT_URL=ws://127.0.0.1:7880
export LIVEKIT_API_KEY=devkey
export LIVEKIT_API_SECRET=secret
```

Run a subscriber in another terminal to verify the negotiated codec and decoder
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
30 access units per second. A healthy subscriber shows a matching
`Subscribed to video track` codec and rising decoded-frame counts with low loss
and no repeated PLI loop.

### GStreamer `gstappsink` source

Exercises
`GStreamer videotestsrc -> encoder -> appsink -> GStreamerAppSinkEncodedSource -> VideoCaptureTrack`.

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

For H.265, VP8, VP9, or AV1, change `--codec` accordingly.

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

### TCP `tcpsink` source

Exercises
`GStreamer videotestsrc -> encoder -> tcpserversink -> TcpEncodedSource -> VideoCaptureTrack`.
The `tcpsink` source connects to a TCP producer such as the fixture script's
GStreamer `tcpserversink`.

Start the producer, then publish:

```sh
examples/preencode_publish/scripts/run-tcp-test-source.sh --codec h264 --port 5000
```

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

For H.265, use `--codec h265` in both commands. For VP8, VP9, or AV1, use the
same script with the matching `--codec` and add `--tcp-format auto` to the
publisher, which selects RTP automatically:

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

### Shared-memory `shmsink` source

Exercises
`GStreamer videotestsrc -> encoder -> shmsink -> shmsrc -> GStreamerAppSinkEncodedSource -> VideoCaptureTrack`.

Start the producer, then publish by connecting the `shmsink` source to that
socket:

```sh
examples/preencode_publish/scripts/run-shm-test-source.sh \
  --codec h264 \
  --socket-path /tmp/livekit-preencode-h264.shm
```

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

For H.265, VP8, or VP9, use the same command shape with the matching `--codec`
(and a different socket path if desired). 

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

Exercises
`GStreamer videotestsrc -> encoder -> RTP payloader -> gst-rtsp-server -> RtspEncodedSource -> VideoCaptureTrack`.

Start the RTSP server (the script uses the `test-launch` tool from
`gst-rtsp-server` and serves `/test`), then publish:

```sh
examples/preencode_publish/scripts/run-rtsp-test-source.sh --codec h264 --port 8555
```

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

For H.265, use `--codec h265` in both commands. For VP8, VP9, or AV1, use the
matching `--codec` in both commands; the RTSP fixture switches to `rtpvp8pay`,
`rtpvp9pay`, or `rtpav1pay` automatically.

Publisher-side success signs are a successful room connection, a
`Published pre-encoded ... track at 1280x720` log line, and diagnostics near
30 access units per second.
