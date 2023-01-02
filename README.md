# LiveKit: Native SDK

> **Warning**
> This SDK is not yet stable, the API may change and the supported features are limited.
> All feedbacks/contributions are appreciated. You can create issues or discuss with us on the #rust-developer-preview channel in our [Slack](https://livekit.io/join-slack)

## Features

- [x] Downstream tracks ( VP8, Software decoder only )
- [x] Cross-platform ( currently tested on Windows & MacOS )
- [ ] Upstream tracks 
- [ ] Adaptive Streaming
- [ ] Dynacast
- [ ] Simulcast
- [ ] Hardware video enc/dec
   - NvEnc for Windows
   - VideoToolbox for MacOS/iOS

## Crates
- `livekit-core`: LiveKit protocol implementation
- `livekit-utils`: Shared utilities between our crates
- `livekit-ffi`: Use `livekit-core` on foreign languages
- `livekit-webrtc`: Safe Rust bindings to libwebrtc 

## Design Goals
- Be used as a common core across our native SDKs
- Create Client SDKs more quickly for different languages
- Be used as a standalone cross-platform SDK

## Getting started
Tokio is required to use the SDK, we have plan to make the async executor agnostic
### Connecting to a Room and listen to events:
```rust
#[tokio::main]
async fn main() -> Result<()> {
   let (room, room_events) = Room::connect(&url, &token).await?;
   
   while let Some(event) = room_events.recv().await {
      match event {
         RoomEvent::TrackSubscribed { track, publication, participant } => {
            // ...
         }
         _ => {}
      }
   }
   
   Ok(())
}
```

## Examples
We made [simple room](https://github.com/livekit/client-sdk-native/tree/main/examples/simple_room) demo using all features of the SDK. We render videos using wgpu and egui.
