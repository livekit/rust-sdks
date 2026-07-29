# livekit-capture

Capture sources and helpers for publishing video with the LiveKit Rust SDK.
The optional `gstreamer` feature turns a GStreamer `appsink` into an encoded
ingest source; the `demo` feature adds a synthetic pixel source for testing.

## Library entry points

- `pixel::PixelVideoSource` and `encoded::EncodedVideoSource` — the traits a
  capture backend implements: pixel sources yield libwebrtc `VideoFrame`s
  (any `VideoBuffer`, CPU or native, with no intermediate copy) published
  through the WebRTC encoder; encoded sources produce crate-owned access
  units published as passthrough. Both traits are object-safe and
  implemented for `Box<dyn ...>`, so sources can be constructed dynamically
  and driven through the same pumps. The crate owns a type only where it
  adds semantics (`encoded::EncodedAccessUnit` and the parsing/validation
  vocabulary); elsewhere livekit's types are used directly.
- `pixel::PixelVideoPump<S>` and `encoded::EncodedVideoPump<S>` — bridge a
  source into a publishable RTC track: each builds the matching
  `NativeVideoSource`, derives publish options (`EncodedVideoPump` selects
  the passthrough encoder), and runs the capture loop on a plain thread.
  Encoded pumps forward downstream keyframe and rate-control requests back
  to the source and drop pre-roll deltas until the first keyframe. Both
  spawn into the same `pump::RunningPump`, so an application supervises
  running pumps of either kind uniformly (`stop()`, `join_async()`, stats);
  the `pump` module holds this shared machinery.
- `sources::gstreamer::GStreamerVideoSource` — built solely from
  configuration (`GStreamerVideoSourceConfig`: launch description, codec,
  resolution, optional rate-control binding). The source owns its pipeline:
  it is started at construction, construction fails loudly on pipeline
  problems, bus errors surface as source errors, and the pipeline stops when
  the source is dropped. `encoded_caps_string` remains the single per-codec
  caps table for writing producer pipelines.

## GStreamer ingest

`GStreamerVideoSource` implements `EncodedVideoSource` on top of a pipeline
whose `appsink` (named `lk_appsink`, or attached automatically to one
unlinked encoded pad) produces H.264 (Annex-B or AVC), H.265 Annex-B, VP8,
VP9, or AV1 access units. Drive it with an `EncodedVideoPump`, which builds
the encoded RTC source, derives the passthrough publish options, and
forwards keyframe requests (answered with a `GstForceKeyUnit` upstream
event) and rate-control targets back to the pipeline. Passthrough is
single-layer (`L1T1`); access units carrying other layering metadata are
rejected.
