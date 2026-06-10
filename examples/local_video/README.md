# local_video

Examples demonstrating capturing frames from a local camera video and publishing to LiveKit, listing camera capabilities, subscribing to render video in a window, and showing a low-latency clock for measurement.

**Note:** These examples are intended for **desktop platforms only** (macOS, Linux, Windows).
You must enable the `desktop` feature when building or running them.
For smoother local rendering, especially above 720p, run the publisher/subscriber with `cargo run --release`.

- list_devices: enumerate available cameras and their capabilities
- publisher: capture from a selected camera and publish a video track
- subscriber: connect to a room, subscribe to video tracks, and display in a window
- clock: render a high-contrast wall-clock with three millisecond digits and a millisecond grid

LiveKit connection can be provided via flags or environment variables:
- `--url` or `LIVEKIT_URL`
- `--api-key` or `LIVEKIT_API_KEY`
- `--api-secret` or `LIVEKIT_API_SECRET`

Publisher usage:
```
 cargo run -p local_video -F desktop --bin publisher -- --list-cameras
 cargo run -p local_video -F desktop --bin publisher -- --camera-index 0 --room-name demo --identity cam-1
 
 # with explicit LiveKit connection flags
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --simulcast \
   --codec h265 \
   --max-bitrate 1500000 \
   --url https://your.livekit.server \
   --api-key YOUR_KEY \
   --api-secret YOUR_SECRET

 # publish with a user timestamp attached to every frame
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --attach-timestamp

 # publish with timestamp burned into the video and a frame ID in the packet trailer
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --attach-timestamp \
   --burn-timestamp \
   --attach-frame-id

 # publish at a custom resolution and framerate
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --width 1920 \
   --height 1080 \
   --fps 60 \
   --room-name demo \
   --identity cam-1

 # publish a static SMPTE color-bar test pattern (no camera required)
 cargo run -p local_video -F desktop --bin publisher -- \
   --test-pattern \
   --room-name demo \
   --identity test-1

 # publish with end-to-end encryption
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --e2ee-key my-secret-key

 # publish and display the outgoing video locally
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --display-video

 # publish with FlexFEC (video/flexfec-03) forward error correction
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --flex-fec

 # FlexFEC with a fixed 20% protection rate, bursty-loss mask, and at most
 # 4 frames per FEC block (all FEC knobs require --flex-fec)
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --flex-fec \
   --fec-protection-rate 20 \
   --fec-mask-type bursty \
   --fec-max-frames 4
```

List devices usage:
```
 cargo run -p local_video -F desktop --bin list_devices
```

Clock usage:
```
 cargo run -p local_video -F desktop --bin clock
 cargo run --release -p local_video -F desktop --bin clock -- --fullscreen
```

Clock flags:
- `--fullscreen`: Start in borderless fullscreen.
- `--always-on-top`: Keep the clock above normal windows.
- `--no-vsync`: Disable vsync and render as fast as the display backend accepts frames. By default the clock uses WGPU with vsync and a maximum frame latency of 1 to avoid uncapped GPU usage.

The clock draws a 3x9 grid below the time. The top row fills from `0` to `9` for the hundreds-of-milliseconds digit, the middle row for tens of milliseconds, and the bottom row for ones of milliseconds.

Publisher flags (in addition to the common connection flags above):
- `--camera-index <n>`: Camera index to use (default: `0`). Use `--list-cameras` to see available indices.
- `--test-pattern`: Generate a standard SMPTE 75% color-bar test pattern instead of capturing from a camera. `--camera-index` is ignored when this is set; `--width`, `--height`, and `--fps` still control the output resolution and frame rate.
- `--width <px>`: Desired capture width (default: `1280`).
- `--height <px>`: Desired capture height (default: `720`).
- `--fps <n>`: Desired capture framerate (default: `30`).
- `--codec <codec>`: Video codec to use for publishing: `h264`, `h265`, `vp8`, `vp9`, or `av1` (default: `h264`). H.265 falls back to H.264 on failure.
- `--simulcast`: Publish simulcast video (multiple layers when the resolution is large enough).
- `--max-bitrate <bps>`: Max video bitrate for the main (highest) layer in bits per second (e.g. `1500000`).
- `--attach-timestamp`: Attach the current wall-clock time (microseconds since UNIX epoch) as the user timestamp on each published frame. The subscriber can display this to measure end-to-end latency.
- `--burn-timestamp`: Burn the attached timestamp into the video frame as a visible overlay. Has no effect unless `--attach-timestamp` is also set.
- `--attach-frame-id`: Attach a monotonically increasing frame ID to each published frame via the packet trailer. The subscriber displays this in the timestamp overlay when `--display-timestamp` is used.
- `--display-video`: Open a window that displays the video frames being published.
- `--display-timing`: Burn publisher timing metrics into the local preview window. Requires `--display-video`.
- `--e2ee-key <key>`: Enable end-to-end encryption with the given shared key. The subscriber must use the same key to decrypt.
- `--flex-fec`: Enable FlexFEC (`video/flexfec-03`) forward error correction for the published video track.
- `--fec-protection-rate <0-100>`: Fixed FEC protection rate in percent, replacing libwebrtc's adaptive, loss-based rate. Requires `--flex-fec`.
- `--fec-mask-type <random|bursty>`: FEC packet mask type (protection pattern): `random` targets uniform loss, `bursty` targets consecutive loss. Requires `--flex-fec`.
- `--fec-max-frames <n>`: Maximum number of video frames protected by a single FEC block. Defaults to `1` (each frame protected independently, lowest latency) when `--flex-fec` is set. Requires `--flex-fec`.

FlexFEC notes:
- `--flex-fec` sets process-wide WebRTC field trials (`WebRTC-FlexFEC-03` / `WebRTC-FlexFEC-03-Advertised`) before connecting, so it applies to every track published by the process.
- Without `--fec-protection-rate`, libwebrtc adapts the FEC rate to observed packet loss and may send almost no FEC packets on loss-free links. The fixed rate is applied on top of the encoder target, so keep it moderate on bandwidth-constrained links.
- With `--simulcast`, libwebrtc protects only the primary (first) simulcast stream.
- FEC only flows if the SFU accepts `flexfec-03` in its answer; otherwise the track publishes normally without FEC. Verify negotiation by running with `RUST_LOG=livekit=debug` and looking for `flexfec-03` and `a=ssrc-group:FEC-FR` in the `sending publisher offer` log line.

Subscriber usage:
```
 # relies on env vars LIVEKIT_URL, LIVEKIT_API_KEY, LIVEKIT_API_SECRET
 cargo run -p local_video -F desktop --bin subscriber -- --room-name demo --identity viewer-1

 # or pass credentials via flags
 cargo run -p local_video -F desktop --bin subscriber -- \
   --room-name demo \
   --identity viewer-1 \
   --url https://your.livekit.server \
   --api-key YOUR_KEY \
   --api-secret YOUR_SECRET

 # subscribe to a specific participant's video only
 cargo run -p local_video -F desktop --bin subscriber -- \
   --room-name demo \
   --identity viewer-1 \
   --participant alice

 # display timestamp overlay (requires publisher to use --attach-timestamp)
 cargo run -p local_video -F desktop --bin subscriber -- \
   --room-name demo \
   --identity viewer-1 \
   --display-timestamp

 # subscribe with end-to-end encryption (must match publisher's key)
 cargo run -p local_video -F desktop --bin subscriber -- \
   --room-name demo \
   --identity viewer-1 \
   --e2ee-key my-secret-key
```

Subscriber flags (in addition to the common connection flags above):
- `--participant <identity>`: Only subscribe to video tracks from the specified participant.
- `--display-timestamp`: Show a top-left overlay with frame ID, the publisher's timestamp, the subscriber's current time, and the computed end-to-end latency. Timestamp fields require the publisher to use `--attach-timestamp`; frame ID requires `--attach-frame-id`.
- `--e2ee-key <key>`: Enable end-to-end decryption with the given shared key. Must match the key used by the publisher.

Notes:
- If the active video track is unsubscribed or unpublished, the app clears its state and will automatically attach to the next matching video track when it appears.
- For E2EE to work, both publisher and subscriber must specify the same `--e2ee-key` value. If the keys don't match, the subscriber will not be able to decode the video.
- The timestamp overlay updates at ~2 Hz so the latency value is readable rather than flickering every frame.
