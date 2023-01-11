# 📹🎙️🦀 Rust Client SDK for LiveKit

[![crates.io](https://img.shields.io/crates/v/livekit.svg)](https://crates.io/crates/livekit)
[![Tests & Build](https://github.com/livekit/client-sdk-native/actions/workflows/rust.yml/badge.svg?branch=main)](https://github.com/livekit/client-sdk-native/actions/workflows/rust.yml)

⚠️ Warning

> This SDK is currently in Developer Preview mode and not ready for production use. There will be bugs and APIs may change during this period.
>
> We welcome and appreciate any feedback or contributions. You can create issues here or chat live with us in the #rust-developer-preview channel within the [LiveKit Community Slack](https://livekit.io/join-slack).

## Features

- [x] Receiving tracks
- [x] Cross-platform ( currently tested on Windows & MacOS )
- [ ] Publishing tracks
- [ ] Adaptive Streaming
- [ ] Dynacast
- [ ] Simulcast
- [ ] Hardware video enc/dec
  - [ ] NvEnc for Windows
  - [x] VideoToolbox for MacOS/iOS

## Crates

- `livekit-core`: LiveKit protocol implementation
- `livekit-utils`: Shared utilities between our crates
- `livekit-ffi`: Bindings for other languages. Uses `livekit-core`.
- `livekit-webrtc`: Safe Rust bindings to libwebrtc
- `webrtc-sys`: Unsafe bindings to libwebrtc

## Motivation and Design Goals

LiveKit aims to provide an open source, end-to-end WebRTC stack that works everywhere. We have two goals in mind with this SDK:

1. Build a standalone, cross-platform LiveKit client SDK for Rustaceans.
2. Build a common core for other platform-specific SDKs (e.g. Unity, Unreal, iOS, Android)

Regarding (2), we've already developed a number of [client SDKs](https://github.com/livekit?q=client-sdk&type=all) for several platforms and encountered a few challenges in the process:

- There's a significant amount of business/control logic in our signaling protocol and WebRTC. Currently, this logic needs to be implemented in every new platform we support.
- Interactions with media devices and encoding/decoding are specific to each platform and framework.
- For multi-platform frameworks (e.g. Unity, Flutter, React Native), the aforementioned tasks proved to be extremely painful.

Thus, we posited a Rust SDK, something we wanted build anyway, encapsulating all our business logic and platform-specific APIs into a clean set of abstractions, could also serve as the foundation for our other SDKs!

We'll first use it as a basis for our Unity SDK (under development), but over time, it will power our other SDKs, as well.

## Getting started

Currently, Tokio is required to use this SDK, however we plan to make the async executor runtime agnostic.

### Connecting to a Room and listen to events:

```rust
use livekit::prelude::*;

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

We made a [simple room demo](https://github.com/livekit/client-sdk-native/tree/main/examples/simple_room) leveraging all the current SDK features. Videos are rendered using wgpu and egui.

![](https://github.com/livekit/client-sdk-rust/blob/main/examples/images/simple-room-demo.gif)

## FAQ

### Do you plan to offer a C/C++ SDK?

Yes! In fact, we also plan to release an SDK for C++ in the coming months. It, like our other platform-specific SDKs, will use the Rust SDK. 🙂

### Did you consider C/C++ as your common core?

Yes. We chose Rust over C++ for a few reasons:

- Rust's ownership model and thread-safety leads to fewer crashes/issues.
- Rust's build system requires less configuration and is easier to work with.
- While we love C/C++, it's a bit nicer to write code in Rust.
- Rust has a rich ecosystem of tools (e.g. websockets, async executor).
- Having the WebAssembly target will be useful down the road, C++ has Emscripten but it's a bit harder to set up and doesn't yet have WebRTC support.

### Did you look at Arcas for libwebrtc bindings?

Yes. Our build system is inspired by LBL's work! Given that some of our logic (e.g. hardware decoder code) is in C++ and that we may need to bridge more/different things than Arcas, we decided it was better to have our own bindings for full control.
