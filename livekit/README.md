# LiveKit Client SDK

The official client SDK for [LiveKit](https://livekit.com).

Use this SDK to add realtime video, audio and data features to your Rust app.

## Use native Media over QUIC

A desktop or server participant can publish and receive video and DataTracks over native QUIC while sharing a room with WebRTC participants. Enable the `moq` Cargo feature on `livekit` and a WebSocket TLS feature such as `rustls-tls-native-roots` for secure signaling.

```rust
use livekit::prelude::{Room, RoomOptions, RoomTransport};

let mut options = RoomOptions::default();
options.transport = RoomTransport::Moq;
let (room, events) = Room::connect(signaling_url, participant_token, options).await?;
```

Use the same signaling URL, access token and track APIs as for WebRTC. The server supplies the QUIC endpoint during the join. For named data, use `publish_data_track` and subscribe to the remote DataTrack. For server configuration and the third-party wire profile, see `docs/moq.md` in the matching LiveKit server checkout. [SDK room interoperability tests](tests/moq_test.rs)

For session setup, media framing, DataTrack ordering and reconnect behavior, see the [Rust SDK MoQ integration guide](docs/moq.md).
