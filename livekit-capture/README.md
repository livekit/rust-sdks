# LiveKit Capture

> [!IMPORTANT]
> This crate is currently in Developer Preview mode and not ready for production use.
> There may be bugs, and APIs and configuration options are subject to change during this period.

This crate provides video capture sources and the pumps that publish them
with the LiveKit [Rust SDK](../livekit/README.md). Pick a ready-made source,
or implement one small trait to add your own. The application runs and
supervises every source the same way.

## Source and pump

Two concepts make up the crate. Video reaches a LiveKit track in one of two
forms, so the source and the pump each have two variants.

**A source** produces video, one blocking call at a time. Use a ready-made
source (see [Sources](#sources)), or implement the trait to add your own:

- A `pixel::PixelVideoSource` produces raw `VideoFrame`s — from a camera, for
  example. The WebRTC encoder encodes them.
- An `encoded::EncodedVideoSource` produces access units that are already
  encoded — from an encoding pipeline, for example. The SDK sends them to the
  wire without re-encoding (passthrough). When frames are encoded upstream,
  this removes an extra decode and encode step and lowers latency.

**A pump** connects one source to an RTC video source:
`pixel::PixelVideoPump<S>` or `encoded::EncodedVideoPump<S>`. It builds the
matching RTC video source, derives the publish options, and runs the capture
loop. Spawn a pump onto a dedicated thread and it becomes a
`pump::RunningPump`. Both pump kinds spawn into the same type, so an
application supervises them the same way. Stop a running pump from any
thread through its stop handle.

Sources block, so the pumps run synchronous code on plain threads.

## Publishing a track

A pump supplies the RTC source and the publish options, so publication is the
same for either path.

```rust
let pump = PixelVideoPump::new(PatternVideoSource::new(config).await?);

let track = LocalVideoTrack::create_video_track("pattern", pump.rtc_source());
let options = pump.publish_options();
room.local_participant().publish_track(LocalTrack::Video(track), options).await?;

let running = pump.spawn()?;

// On shutdown:
let stats = running.stop_and_join_async().await?;
```

## Sources

Each source lives in its own module under `sources`, behind a Cargo feature
named `source-<module>`. Each module documents its source.

| Feature            | Source                 | Kind    |
| ------------------ | ---------------------- | ------- |
| `source-device`    | `DeviceVideoSource`    | pixel   |
| `source-gstreamer` | `GStreamerVideoSource` | encoded |
| `source-rtsp`      | `RtspVideoSource`      | encoded |
| `source-pattern`   | `PatternVideoSource`   | pixel   |
| `source-clock`     | `ClockVideoSource`     | pixel   |

`source-rtsp-tls` extends `RtspVideoSource` with `rtsps://` support (RTSP
over TLS 1.2+). Certificates are verified against the system roots by
default; cameras with self-signed certificates can opt out per source.
