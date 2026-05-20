# local_video

Three examples demonstrating capturing frames from a local camera video and publishing to LiveKit, listing camera capabilities, and subscribing to render video in a window.

**Note:** These examples are intended for **desktop platforms only** (macOS, Linux, Windows).
You must enable the `desktop` feature when building or running them.
For smoother local rendering, especially above 720p, run the publisher/subscriber with `cargo run --release`.

- list_devices: enumerate available cameras and their capabilities
- publisher: capture from a selected camera and publish a video track
- subscriber: connect to a room, subscribe to video tracks, and display each track in its own window

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
   --h265 \
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
```

List devices usage:
```
 cargo run -p local_video -F desktop --bin list_devices
```

Publisher flags (in addition to the common connection flags above):
- `--camera-index <n>`: Camera index to use (default: `0`). Use `--list-cameras` to see available indices.
- `--test-pattern`: Generate a standard SMPTE 75% color-bar test pattern instead of capturing from a camera. `--camera-index` is ignored when this is set; `--width`, `--height`, and `--fps` still control the output resolution and frame rate.
- `--width <px>`: Desired capture width (default: `1280`).
- `--height <px>`: Desired capture height (default: `720`).
- `--fps <n>`: Desired capture framerate (default: `30`).
- `--h265`: Use H.265/HEVC encoding if supported (falls back to H.264 on failure).
- `--simulcast`: Publish simulcast video (multiple layers when the resolution is large enough).
- `--max-bitrate <bps>`: Max video bitrate for the main (highest) layer in bits per second (e.g. `1500000`).
- `--attach-timestamp`: Attach the current wall-clock time (microseconds since UNIX epoch) as the user timestamp on each published frame. The subscriber can display this to measure end-to-end latency.
- `--burn-timestamp`: Burn the attached timestamp into the video frame as a visible overlay. Has no effect unless `--attach-timestamp` is also set.
- `--attach-frame-id`: Attach a monotonically increasing frame ID to each published frame via the packet trailer. The subscriber displays this in the timestamp overlay when `--display-timestamp` is used.
- `--display-video`: Open a window that displays the video frames being published.
- `--display-timing`: Burn publisher timing metrics into the local preview window. Requires `--display-video`.
- `--e2ee-key <key>`: Enable end-to-end encryption with the given shared key. The subscriber must use the same key to decrypt.

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
- The subscriber opens a separate window for every video track it is subscribed to. A small status panel in the main window shows the room, identity, filter, and the list of currently-displayed tracks.
- Closing a track's window unsubscribes from that publication. The window reappears automatically if the publisher republishes the track (or publishes a new one) and it still matches the optional `--participant` filter.
- If the active video track is unsubscribed or unpublished, its window is closed automatically.
- For E2EE to work, both publisher and subscriber must specify the same `--e2ee-key` value. If the keys don't match, the subscriber will not be able to decode the video.
- The timestamp overlay updates at ~2 Hz so the latency value is readable rather than flickering every frame.
