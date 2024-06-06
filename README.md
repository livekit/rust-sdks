<!--BEGIN_BANNER_IMAGE-->

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="/.github/banner_dark.png">
  <source media="(prefers-color-scheme: light)" srcset="/.github/banner_light.png">
  <img style="width:100%;" alt="The LiveKit icon, the name of the repository and some sample code in the background." src="https://raw.githubusercontent.com/livekit/rust-sdks/main/.github/banner_light.png">
</picture>

<!--END_BANNER_IMAGE-->

# 📹🎙️🦀 Rust Client SDK for LiveKit

<!--BEGIN_DESCRIPTION-->
Use this SDK to add real-time video, audio and data features to your Rust app. By connecting to a self- or cloud-hosted <a href="https://livekit.io/">LiveKit</a> server, you can quickly build applications like interactive live streaming or video calls with just a few lines of code.
<!--END_DESCRIPTION-->

[![crates.io](https://img.shields.io/crates/v/livekit.svg)](https://crates.io/crates/livekit)
[![livekit docs.rs](https://img.shields.io/docsrs/livekit)](https://docs.rs/livekit/latest/)
[![Builds](https://github.com/livekit/rust-sdks/actions/workflows/builds.yml/badge.svg?branch=main)](https://github.com/livekit/rust-sdks/actions/workflows/builds.yml)
[![Tests](https://github.com/livekit/rust-sdks/actions/workflows/tests.yml/badge.svg?branch=main)](https://github.com/livekit/rust-sdks/actions/workflows/tests.yml)

## Features

- [x] Receiving tracks
- [x] Publishing tracks
- [x] Data channels
- [x] Simulcast
- [ ] SVC codecs (AV1/VP9)
- [ ] Adaptive Streaming
- [ ] Dynacast
- [x] Hardware video enc/dec
  - [x] VideoToolbox for MacOS/iOS
- Supported Platforms
  - [x] Windows
  - [x] MacOS
  - [x] Linux
  - [x] iOS
  - [x] Android

## Crates

- `livekit-api`: Server APIs and auth token generation
- `livekit`: LiveKit real-time SDK
- `livekit-ffi`: Internal crate, used to generate bindings for other languages
- `livekit-protocol`: LiveKit protocol generated code

When adding the SDK as a dependency to your project, make sure to add the
[necessary `rustflags`](https://github.com/livekit/rust-sdks/blob/main/.cargo/config)
to your cargo config, otherwise linking may fail.

Also, please refer to the list of the [supported platform toolkits](https://github.com/livekit/rust-sdks/blob/main/.github/workflows/builds.yml).

## Getting started

Currently, Tokio is required to use this SDK, however we plan to make the async executor runtime agnostic.

## Using Server API

### Generating an access token

```rust
use livekit_api::access_token;
use std::env;

fn create_token() -> Result<String, access_token::AccessTokenError> {
    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity("rust-bot")
        .with_name("Rust Bot")
        .with_grants(access_token::VideoGrants {
             room_join: true,
             room: "my-room".to_string(),
             ..Default::default()
        })
        .to_jwt();
    return token
}
```

### Creating a room with RoomService API

```rust
use livekit_api::services::room::{CreateRoomOptions, RoomClient};

#[tokio::main]
async fn main() {
    let room_service = RoomClient::new("http://localhost:7880").unwrap();

    let room = room_service
        .create_room("my_room", CreateRoomOptions::default())
        .await
        .unwrap();

    println!("Created room: {:?}", room);
}
```

## Using Real-time SDK

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
...
use futures::StreamExt; // this trait is required for iterating on audio & video frames
use livekit::prelude::*;

match event {
    RoomEvent::TrackSubscribed { track, publication, participant } => {
        match track {
            RemoteTrack::Audio(audio_track) => {
                let rtc_track = audio_track.rtc_track();
                let mut audio_stream = NativeAudioStream::new(rtc_track);
                tokio::spawn(async move {
                    // Receive the audio frames in a new task
                    while let Some(audio_frame) = audio_stream.next().await {
                        log::info!("received audio frame - {audio_frame:#?}");
                    }
                });
            },
            RemoteTrack::Video(video_track) => {
                let rtc_track = video_track.rtc_track();
                let mut video_stream = NativeVideoStream::new(rtc_track);
                tokio::spawn(async move {
                    // Receive the video frames in a new task
                    while let Some(video_frame) = video_stream.next().await {
                        log::info!("received video frame - {video_frame:#?}");
                    }
                });
            },
        }
    },
    _ => {}
}
```

## Examples

![](https://github.com/livekit/rust-sdks/blob/main/examples/images/simple-room-demo.gif)

- [basic room](https://github.com/livekit/rust-sdks/tree/main/examples/basic_room): simple example connecting to a room.
- [wgpu_room](https://github.com/livekit/rust-sdks/tree/main/examples/wgpu_room): complete example app with video rendering using wgpu and egui.
- [mobile](https://github.com/livekit/rust-sdks/tree/main/examples/mobile): mobile app targeting iOS and Android
- [play_from_disk](https://github.com/livekit/rust-sdks/tree/main/examples/play_from_disk): publish audio from a wav file
- [save_to_disk](https://github.com/livekit/rust-sdks/tree/main/examples/save_to_disk): save received audio to a wav file

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

<!--BEGIN_REPO_NAV-->
<br/><table>
<thead><tr><th colspan="2">LiveKit Ecosystem</th></tr></thead>
<tbody>
<tr><td>Real-time SDKs</td><td><a href="https://github.com/livekit/components-js">React Components</a> · <a href="https://github.com/livekit/client-sdk-js">Browser</a> · <a href="https://github.com/livekit/client-sdk-swift">iOS/macOS</a> · <a href="https://github.com/livekit/client-sdk-android">Android</a> · <a href="https://github.com/livekit/client-sdk-flutter">Flutter</a> · <a href="https://github.com/livekit/client-sdk-react-native">React Native</a> · <b>Rust</b> · <a href="https://github.com/livekit/node-sdks">Node.js</a> · <a href="https://github.com/livekit/python-sdks">Python</a> · <a href="https://github.com/livekit/client-sdk-unity-web">Unity (web)</a> · <a href="https://github.com/livekit/client-sdk-unity">Unity (beta)</a></td></tr><tr></tr>
<tr><td>Server APIs</td><td><a href="https://github.com/livekit/node-sdks">Node.js</a> · <a href="https://github.com/livekit/server-sdk-go">Golang</a> · <a href="https://github.com/livekit/server-sdk-ruby">Ruby</a> · <a href="https://github.com/livekit/server-sdk-kotlin">Java/Kotlin</a> · <a href="https://github.com/livekit/python-sdks">Python</a> · <b>Rust</b> · <a href="https://github.com/agence104/livekit-server-sdk-php">PHP (community)</a></td></tr><tr></tr>
<tr><td>Agents Frameworks</td><td><a href="https://github.com/livekit/agents">Python</a> · <a href="https://github.com/livekit/agent-playground">Playground</a></td></tr><tr></tr>
<tr><td>Services</td><td><a href="https://github.com/livekit/livekit">Livekit server</a> · <a href="https://github.com/livekit/egress">Egress</a> · <a href="https://github.com/livekit/ingress">Ingress</a> · <a href="https://github.com/livekit/sip">SIP</a></td></tr><tr></tr>
<tr><td>Resources</td><td><a href="https://docs.livekit.io">Docs</a> · <a href="https://github.com/livekit-examples">Example apps</a> · <a href="https://livekit.io/cloud">Cloud</a> · <a href="https://docs.livekit.io/oss/deployment">Self-hosting</a> · <a href="https://github.com/livekit/livekit-cli">CLI</a></td></tr>
</tbody>
</table>
<!--END_REPO_NAV-->
