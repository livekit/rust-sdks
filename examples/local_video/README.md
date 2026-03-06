# local_video

Three examples demonstrating capturing frames from a local camera video and publishing to LiveKit, listing camera capabilities, and subscribing to render video in a window.

**Note:** These examples are intended for **desktop platforms only** (macOS, Linux, Windows).
You must enable the `desktop` feature when building or running them.

- list_devices: enumerate available cameras and their capabilities
- publisher: capture from a selected camera and publish a video track
- subscriber: connect to a room, subscribe to video tracks, and display in a window

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

 # publish with end-to-end encryption
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --e2ee-key my-secret-key
```

List devices usage:
```
 cargo run -p local_video -F desktop --bin list_devices
```

Publisher flags (in addition to the common connection flags above):
- `--h265`: Use H.265/HEVC encoding if supported (falls back to H.264 on failure).
- `--simulcast`: Publish simulcast video (multiple layers when the resolution is large enough).
- `--max-bitrate <bps>`: Max video bitrate for the main (highest) layer in bits per second (e.g. `1500000`).
- `--attach-timestamp`: Attach the current wall-clock time (microseconds since UNIX epoch) as the user timestamp on each published frame. The subscriber can display this to measure end-to-end latency.
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
- `--display-timestamp`: Show a top-left overlay with the publisher's timestamp, the subscriber's current time, and the computed end-to-end latency. Requires the publisher to use `--attach-timestamp`.
- `--e2ee-key <key>`: Enable end-to-end decryption with the given shared key. Must match the key used by the publisher.

Notes:
- If the active video track is unsubscribed or unpublished, the app clears its state and will automatically attach to the next matching video track when it appears.
- For E2EE to work, both publisher and subscriber must specify the same `--e2ee-key` value. If the keys don't match, the subscriber will not be able to decode the video.
- The timestamp overlay updates at ~2 Hz so the latency value is readable rather than flickering every frame.
