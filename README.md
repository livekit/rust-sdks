# LiveKit: Native SDK
![crates.io](https://img.shields.io/crates/v/livekit.svg)
[![Tests & Build](https://github.com/livekit/client-sdk-native/actions/workflows/rust.yml/badge.svg?branch=main)](https://github.com/livekit/client-sdk-native/actions/workflows/rust.yml)
> **Warning**
> This SDK is a developer preview and is not ready for production use. There will be bugs and the APIs may change during this period.
> All feedbacks/contributions are appreciated. You can create issues or discuss with us on the #rust-developer-preview channel in our [Slack](https://livekit.io/join-slack)

## Features

- [x] Receiving tracks ( VP8, Software decoder only )
- [x] Cross-platform ( currently tested on Windows & MacOS )
- [ ] Publishing tracks
- [ ] Adaptive Streaming
- [ ] Dynacast
- [ ] Simulcast
- [ ] Hardware video enc/dec
  - [ ] NvEnc for Windows
  - [ ] VideoToolbox for MacOS/iOS

## Crates

- `livekit-core`: LiveKit protocol implementation
- `livekit-utils`: Shared utilities between our crates
- `livekit-ffi`: Use `livekit-core` on foreign languages
- `livekit-webrtc`: Safe Rust bindings to libwebrtc
- `webrtc-sys`: Unsafe bindings to libwebrtc

## Motivation and Design Goals

At LiveKit, we've developed a number of client SDKs for different platforms. This
is necessary to our goal of providing an end-to-end WebRTC stack for every platform. However,
we've encountered a few challenges during this process:

- there's significant of business/control logic with our signaling protocol and WebRTC. currently they are re-written for each platform that we support
- interactions with media devices and encoding/decoding are platform and framework specific
- doing both of the above for multi-platform frameworks (like Unity, Flutter, and React-Native) proved to be extremely painful

We would like this SDK to:

- Encapsulate all of the business logic and platform-specific APIs into a clean
- Standalone cross-platform, native SDK for Rust and C/C++
- Serve as a common core for other platform-specific SDKs (i.e. Unity, iOS, Android)

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

### Receive video frames of a subscribed track

```rust
match event {
   RoomEvent::TrackSubscribed { track, publication, participant } => {
      if let RemoteTrackHandle::Video(video_track) => {
          let rtc_track = video_track.rtc_track();
          rtc_track.on_frame(Box::new(move |frame, buffer| {
              // Just received a video frame!
              // The buffer is YuvEncoded, you can decode it to ABGR by using our yuv_helper
              // See the simple_room example for the conversion 
          });
      } else {
          // Audio Track..
      }
   }
   _ => {}
}
```

## Examples

We made [simple room](https://github.com/livekit/client-sdk-native/tree/main/examples/simple_room) demo using all features of the SDK. We render videos using wgpu and egui.
