# local_video

Two examples demonstrating capturing frames from a local camera video and publishing to LiveKit, and subscribing to render video in a window.

- publisher: capture from a selected camera and publish a video track
- subscriber: connect to a room, subscribe to video tracks, and display in a window

LiveKit connection can be provided via flags or environment variables:
- `--url` or `LIVEKIT_URL`
- `--api-key` or `LIVEKIT_API_KEY`
- `--api-secret` or `LIVEKIT_API_SECRET`

Publisher usage:
```
 cargo run -p local_video --bin publisher -- --list-cameras
 cargo run -p local_video --bin publisher -- --camera-index 0 --room-name demo --identity cam-1
 
 # with explicit LiveKit connection flags
 cargo run -p local_video --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --h265 \
   --max-bitrate 1500000 \
   --url https://your.livekit.server \
   --api-key YOUR_KEY \
   --api-secret YOUR_SECRET
```

Publisher flags (in addition to the common connection flags above):
- `--h265`: Use H.265/HEVC encoding if supported (falls back to H.264 on failure).
- `--max-bitrate <bps>`: Max video bitrate for the main layer in bits per second (e.g. `1500000`).

Subscriber usage:
```
 # relies on env vars LIVEKIT_URL, LIVEKIT_API_KEY, LIVEKIT_API_SECRET
 cargo run -p local_video --bin subscriber -- --room-name demo --identity viewer-1

 # or pass credentials via flags
 cargo run -p local_video --bin subscriber -- \
   --room-name demo \
   --identity viewer-1 \
   --url https://your.livekit.server \
   --api-key YOUR_KEY \
    --api-secret YOUR_SECRET

  # subscribe to a specific participant's video only
  cargo run -p local_video --bin subscriber -- \
    --room-name demo \
    --identity viewer-1 \
    --participant alice
```

Notes:
- `--participant` limits subscription to video tracks from the specified participant identity.
- If the active video track is unsubscribed or unpublished, the app clears its state and will automatically attach to the next matching video track when it appears.
