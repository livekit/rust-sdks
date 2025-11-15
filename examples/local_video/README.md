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
   --url https://your.livekit.server \
   --api-key YOUR_KEY \
   --api-secret YOUR_SECRET
```

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
```
