# LiveKit Capture

Video capture sources, and the machinery that publishes them with the LiveKit
[Rust SDK](../livekit/README.md). A capture backend implements one small trait. An application then
runs and supervises every backend the same way.

## Source, pump, running pump

Three concepts make up the crate. Video reaches a LiveKit track in one of two
forms, so the source and the pump each have two variants.

**A source** produces frames or access units, one blocking call at a time. It
is the only trait a backend implements. A `pixel::PixelVideoSource` produces
libwebrtc `VideoFrame`s from a device such as a camera, and the WebRTC encoder
encodes them. An `encoded::EncodedVideoSource` produces access units from a
producer that encoded them already, such as an encoding pipeline. Passthrough
sends those to the wire with no re-encode.

**A pump** bridges one source into a publishable RTC track:
`pixel::PixelVideoPump<S>` or `encoded::EncodedVideoPump<S>`. It builds the
matching RTC video source, derives the publish options, and runs the capture
loop.

**A running pump** is a pump on a dedicated thread. Both pump kinds spawn into
the same `pump::RunningPump`, so an application supervises pumps of either kind
the same way.

Sources block, so the pumps are synchronous code on plain threads. Only pump
construction needs the context of the async runtime that drives the SDK.

## Publishing a track

A pump supplies both pieces that the SDK needs, so publication is the same for
either path.

```rust
let pump = PixelVideoPump::new(DemoSource::new(config)?);

let track = LocalVideoTrack::create_video_track("demo", pump.rtc_source());
let options = pump.publish_options();
room.local_participant().publish_track(LocalTrack::Video(track), options).await?;

let running = pump.spawn()?;
let stats = running.stop_and_join_async().await?;
```

## Sources

Each source lives in its own module under `sources`, behind the Cargo feature
of the same name. Its module documents it.

| Feature     | Source                 | Path    |
| ----------- | ---------------------- | ------- |
| `demo`      | `DemoSource`           | pixel   |
| `gstreamer` | `GStreamerVideoSource` | encoded |
