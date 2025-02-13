# ANYbotics forked version of LiveKit Rust SDK
The Rust SDK is using the Google WebRTC repository. The Rust SDK repository has been modified for patching WebRTC. The patches add support for accelerating the encoding process on Intel GPUs, by utilizing the libvpl, vaapi and one of the two Intel GPU runtimes (MediaSDK or Intel VPL GPU RT).
During initialization of WebRTC encoders it is checked whether HW acceleration is possible and if HW acceleration was initialized successfuly. If so, the HW accelerated encoder will take place automatically. Otherwise, the software implementation is used.
**Note**: The design that includes the accelerated encoder implementation is not ideal, however the goal was to modify the Livekit stack as less as possible.

To make use of the accelerated version of WebRTC, we need to build Livekit from source. To achieve this we need to execute the `/webrtc-sys/libwebrtc/build_linux.sh`. This script checks if the WebRTC repository has already been cloned locally. If not, it fetches it, along with all each submodules and applies some Livekit patches on it. Livekit uses a certain WebRTC version, and not the latest one.
To fetch and build WebRTC:
```
./build_linux.sh --arch x64 --profile release
```
In case WebRTC is already present locally, after executing the above script, some warning will be shown regarding the failure of Livekit patches. That's because the patches have already been applied.
Once WebRTC is present locally, we can apply our patches. Navigate to the root of the repository and execute:

```
apply_vpl_patches.sh
```

The implementation of the accelerated encoder is part of the Rust SDK repo, which we have forked, and it is under `libwebrtc-hw`. Ideally the implementation would be part of the WebRTC repository, but it is more complicated. WebRTC repository is huge and includes a lot of sub-modules, which we would have to fork. For now, Rust SDK repository has all the required changes to use Livekit with accelerated encoding.

Once we have WebRTC patched and built, a static library is generated. The next step is to build the Rust part and link it against the generated static library.
Under the root folder execute:
```
cargo clean
cargo build --release
```
Whenever we build again WebRTC, we need to do a cargo clean and rebuild.
At this point a `liblivekit_ffi.so` has been generated, and our application needs to make use of it, to have our accelerated version. To achieve this, we need to expose its path to an environment variable. Along with this environment variable, we need to expose a couple of other ones as well and the application should start in a context that can parse them:
```
export LIVEKIT_URL=wss://192.168.0.6/webrtc/
export LIVEKIT_API_KEY=ads-admin
export LIVEKIT_API_SECRET=livekit-server-secret-for-ads-server
export LIVEKIT_LIB_PATH="/home/integration/gpu_acceleration_ws/anybotics-python-sdks/livekit-rtc/rust-sdks/target/release/liblivekit_ffi.so"
export LD_LIBRARY_PATH=/home/integration/libvpl/_build:$LD_LIBRARY_PATH
export LIBVA_DRIVER_NAME=iHD
export LIBVA_DRIVERS_PATH=/home/integration/media-driver-workspace/build_media/media_driver
```
The above values are indicative.
**LIVEKIT_URL** should include the IP of the desired pc.
**LIVEKIT_API_KEY** and **LIVEKIT_API_SECRET** are the ones that we use to generate a token.
**LIVEKIT_LIB_PATH** should be set accordingly, depending on where we have install the `liblivekit_ffi.so`.
**LD_LIBRARY_PATH** exposes the Intel libvpl and we need to set the path to the installed location.
**LIBVA_DRIVER_NAME** indicates the Intel driver. iHD is the appropriate one for our HW on anymal D.
**LIBVA_DRIVERS_PATH** exposes the path in which we have installed the Intel runtimes (MediaSDK or Intel® VPL)


## Updating patches
To update the patches, navigate to `webrtc-sys/libwebrtc/src` and execute
```
git diff original_commit new_commit --src-prefix=org/webrtc-sys/libwebrtc/src/ --dst-prefix=update/webrtc-sys/libwebrtc/src/ ./path/file > ./../../../libwebrtc-patches/file.patch
```

## Developing and testing
If the development takes place on a PC, we need to start a local server with
```
livekit-server --dev
```
Use the Livekit web client to receive the stream
```
https://meet.livekit.io/?tab=custom
```
Add the required URL of the server.
In case we test on a PC, use the loopback IP
```
ws://localhost:7880
```
In case we test on an anymal use the corresponding IP. The example below is from dobby:
```
wss://192.168.0.6/webrtc/
```

For a token we need to use the Livekit CLI:
```
lk token create --api-key ads-admin --api-secret livekit-server-secret-for-ads-server --join --room dobby --identity test_user --valid-for 24h
```
adapt the arguments accordingly.



# 📹🎙️🦀 Rust Client SDK for LiveKit

<!--BEGIN_DESCRIPTION-->
Use this SDK to add realtime video, audio and data features to your Rust app. By connecting to <a href="https://livekit.io/">LiveKit</a> Cloud or a self-hosted server, you can quickly build applications such as multi-modal AI, live streaming, or video calls with just a few lines of code.
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
[necessary `rustflags`](https://github.com/livekit/rust-sdks/blob/main/.cargo/config.toml)
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
<tr><td>Realtime SDKs</td><td><a href="https://github.com/livekit/components-js">React Components</a> · <a href="https://github.com/livekit/client-sdk-js">Browser</a> · <a href="https://github.com/livekit/components-swift">Swift Components</a> · <a href="https://github.com/livekit/client-sdk-swift">iOS/macOS/visionOS</a> · <a href="https://github.com/livekit/client-sdk-android">Android</a> · <a href="https://github.com/livekit/client-sdk-flutter">Flutter</a> · <a href="https://github.com/livekit/client-sdk-react-native">React Native</a> · <b>Rust</b> · <a href="https://github.com/livekit/node-sdks">Node.js</a> · <a href="https://github.com/livekit/python-sdks">Python</a> · <a href="https://github.com/livekit/client-sdk-unity-web">Unity (web)</a> · <a href="https://github.com/livekit/client-sdk-unity">Unity (beta)</a></td></tr><tr></tr>
<tr><td>Server APIs</td><td><a href="https://github.com/livekit/node-sdks">Node.js</a> · <a href="https://github.com/livekit/server-sdk-go">Golang</a> · <a href="https://github.com/livekit/server-sdk-ruby">Ruby</a> · <a href="https://github.com/livekit/server-sdk-kotlin">Java/Kotlin</a> · <a href="https://github.com/livekit/python-sdks">Python</a> · <b>Rust</b> · <a href="https://github.com/agence104/livekit-server-sdk-php">PHP (community)</a></td></tr><tr></tr>
<tr><td>Agents Frameworks</td><td><a href="https://github.com/livekit/agents">Python</a> · <a href="https://github.com/livekit/agent-playground">Playground</a></td></tr><tr></tr>
<tr><td>Services</td><td><a href="https://github.com/livekit/livekit">LiveKit server</a> · <a href="https://github.com/livekit/egress">Egress</a> · <a href="https://github.com/livekit/ingress">Ingress</a> · <a href="https://github.com/livekit/sip">SIP</a></td></tr><tr></tr>
<tr><td>Resources</td><td><a href="https://docs.livekit.io">Docs</a> · <a href="https://github.com/livekit-examples">Example apps</a> · <a href="https://livekit.io/cloud">Cloud</a> · <a href="https://docs.livekit.io/home/self-hosting/deployment">Self-hosting</a> · <a href="https://github.com/livekit/livekit-cli">CLI</a></td></tr>
</tbody>
</table>
<!--END_REPO_NAV-->
