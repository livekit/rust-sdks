# local_video

Two examples demonstrating capturing frames from a local camera video and publishing to LiveKit, and subscribing to render video in a window.

- publisher: capture from a selected camera and publish a video track
- subscriber: connect to a room, subscribe to video tracks, and display in a window

Environment variables required for both:
- LIVEKIT_URL
- LIVEKIT_API_KEY
- LIVEKIT_API_SECRET

Publisher usage:
```
 cargo run -p local_video --bin publisher -- --list-cameras
 cargo run -p local_video --bin publisher -- --camera-index 0 --room-name demo --identity cam-1
```

Subscriber usage:
```
 cargo run -p local_video --bin subscriber -- --room-name demo --identity viewer-1
```
