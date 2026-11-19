# LiveKit Capture

> [!IMPORTANT]
> This crate is currently in Developer Preview mode and not ready for production use.
> There may be bugs, and APIs and configuration options are subject to change during this period.

This crate provides audio and video capture sources and the pumps that publish them
with the LiveKit [Rust SDK](../livekit/README.md). Pick a ready-made source
or implement your own.

## Source and pump

Sources produce media; pumps deliver it to an RTC source. Audio and video have
separate modules, with a shared pump lifecycle.

**A source** produces media:

- An `audio::PcmAudioSource` produces interleaved signed 16-bit PCM in fixed
  10 ms frames. `audio::AudioPump<S>` feeds it to an RTC audio source.

- A `video::pixel::PixelVideoSource` produces raw `VideoFrame`s — from a camera, for
  example. The WebRTC encoder encodes them.
- An `video::encoded::EncodedVideoSource` produces access units that are already
  encoded — from an encoding pipeline, for example. The SDK sends them to the
  wire without re-encoding (passthrough). When frames are encoded upstream,
  this removes an extra decode and encode step and lowers latency.

**A pump** connects one source to an RTC video source:
`video::pixel::PixelVideoPump<S>` or `video::encoded::EncodedVideoPump<S>`. It builds the
matching RTC video source, derives the publish options, and runs the capture
loop. Spawn a pump and it becomes a `pump::RunningPump`: await it to join,
or stop it from any thread through its stop handle. All pump kinds spawn
into the same type, so an application supervises them the same way.
Spawning is the usual choice; a pump can also `run` on the calling thread,
for an application that creates the capture thread itself.

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
let stats = running.stop_and_join().await?;
```

## Built-in sources

Each source lives in its own module under `audio::sources` or `video::sources`, behind a Cargo feature
named `source-<module>`. Each module documents its source.

| Feature            | Source                 | Kind    |
| ------------------ | ---------------------- | ------- |
| `source-gstreamer` | `GStreamerVideoSource` | encoded |
| `source-gstreamer` | `GStreamerAudioSource` | PCM audio |
| `source-pattern`   | `PatternVideoSource`   | pixel   |
| `source-clock`     | `ClockVideoSource`     | pixel   |

## Custom sources

Implement `audio::PcmAudioSource` for audio, `video::pixel::PixelVideoSource` for raw frames or
`video::encoded::EncodedVideoSource` for pre-encoded access units. A source is
constructed from its own config struct and is driven by the matching pump —
no other integration is needed.

`pump::PumpStats` reports accepted frames and the exit reason. An audio frame
is always 10 ms, regardless of the input backend’s buffer size.

Two conventions keep a source easy to consume:

- **Construct with an `async fn new`**, so an application can build a source
  inline where it publishes. When setup itself blocks — probing a device,
  waiting on a first frame to learn the resolution — run that work in
  `tokio::task::spawn_blocking`.
- **Block freely in the next-frame call.** The pump runs the source on its
  own thread, so waiting there costs the application nothing. Return
  promptly once the stop token fires.
