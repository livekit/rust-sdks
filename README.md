<!--BEGIN_BANNER_IMAGE-->
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="/.github/banner_dark.png">
    <source media="(prefers-color-scheme: light)" srcset="/.github/banner_light.png">
    <img style="width:100%;" alt="The LiveKit icon, the name of the repository and some sample code in the background." src="/.github/banner_light.png">
  </picture>
  <!--END_BANNER_IMAGE-->

# 📹🎙️🦀 Rust Client SDK for LiveKit

<!--BEGIN_DESCRIPTION-->Use this SDK to add real-time video, audio and data features to your Rust app. By connecting to a self- or cloud-hosted <a href="https://livekit.io/">LiveKit</a> server, you can quickly build applications like interactive live streaming or video calls with just a few lines of code.<!--END_DESCRIPTION-->

[![crates.io](https://img.shields.io/crates/v/livekit.svg)](https://crates.io/crates/livekit)
[![Builds](https://github.com/livekit/client-sdk-native/actions/workflows/builds.yml/badge.svg?branch=main)](https://github.com/livekit/client-sdk-native/actions/workflows/builds.yml)
[![Tests](https://github.com/livekit/client-sdk-native/actions/workflows/tests.yml/badge.svg?branch=main)](https://github.com/livekit/client-sdk-native/actions/workflows/tests.yml)

⚠️ Warning

> This SDK is currently in Developer Preview mode and not ready for production use. There will be bugs and APIs may change during this period.
>
> We welcome and appreciate any feedback or contributions. You can create issues here or chat live with us in the #rust-developer-preview channel within the [LiveKit Community Slack](https://livekit.io/join-slack).

## Features

- [x] Receiving tracks
- [x] Cross-platform ( currently tested on Windows & MacOS )
- [x] Data channels
- [x] Publishing tracks
- [ ] Adaptive Streaming
- [ ] Dynacast
- [x] Simulcast
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

### Connect to a Room and listen for events:

```rust
use livekit::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
   let (room, mut room_events) = Room::connect(&url, &token).await?;

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
              // See the basic_room example for the conversion
          });
      } else {
          // Audio Track..
      }
   }
   _ => {}
}
```

## Examples

We made a [basic room demo](https://github.com/livekit/client-sdk-native/tree/main/examples/basic_room) leveraging all the current SDK features. Videos are rendered using wgpu and egui.

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

### Did you look at [Arcas](https://github.com/arcas-io/libwebrtc) for libwebrtc bindings?

Yes. Our build system is inspired by LBL's work! Given that some of our logic (e.g. hardware decoder code) is in C++ and that we may need to bridge more/different things than Arcas, we decided it was better to have our own bindings for full control.

### Did you consider using [webrtc.rs](https://webrtc.rs/) instead of libwebrtc?
Yes! As webrtc.rs matures, we'll eventually migrate to a pure Rust stack. For now, we chose libwebrtc for a few reasons:

- Chrome's adoption and usage means libwebrtc is thoroughly battle-tested.
- webrtc.rs is ported from Pion (which [our SFU](https://github.com/livekit/livekit) is built on) and a better fit for server-side use.
- libwebrtc currently supports more features like encoding/decoding and includes platform-specific code for dealing with media devices.

<!--BEGIN_REPO_NAV-->
<br/><table>
<thead><tr><th colspan="2">LiveKit Ecosystem</th></tr></thead>
<tbody>
<tr><td>Client SDKs</td><td><a href="https://github.com/livekit/components-js">Components</a> · <a href="https://github.com/livekit/client-sdk-js">JavaScript</a> · <a href="https://github.com/livekit/client-sdk-swift">iOS/macOS</a> · <a href="https://github.com/livekit/client-sdk-android">Android</a> · <a href="https://github.com/livekit/client-sdk-flutter">Flutter</a> · <a href="https://github.com/livekit/client-sdk-react-native">React Native</a> · <b>Rust</b> · <a href="https://github.com/livekit/client-sdk-python">Python</a> · <a href="https://github.com/livekit/client-sdk-unity-web">Unity (web)</a> · <a href="https://github.com/livekit/client-sdk-unity">Unity (beta)</a></td></tr><tr></tr>
<tr><td>Server SDKs</td><td><a href="https://github.com/livekit/server-sdk-js">Node.js</a> · <a href="https://github.com/livekit/server-sdk-go">Golang</a> · <a href="https://github.com/livekit/server-sdk-ruby">Ruby</a> · <a href="https://github.com/livekit/server-sdk-kotlin">Java/Kotlin</a> · <a href="https://github.com/agence104/livekit-server-sdk-php">PHP (community)</a> · <a href="https://github.com/tradablebits/livekit-server-sdk-python">Python (community)</a></td></tr><tr></tr>
<tr><td>Services</td><td><a href="https://github.com/livekit/livekit">Livekit server</a> · <a href="https://github.com/livekit/egress">Egress</a> · <a href="https://github.com/livekit/ingress">Ingress</a></td></tr><tr></tr>
<tr><td>Resources</td><td><a href="https://docs.livekit.io">Docs</a> · <a href="https://github.com/livekit-examples">Example apps</a> · <a href="https://livekit.io/cloud">Cloud</a> · <a href="https://docs.livekit.io/oss/deployment">Self-hosting</a> · <a href="https://github.com/livekit/livekit-cli">CLI</a></td></tr>
</tbody>
</table>
<!--END_REPO_NAV-->
