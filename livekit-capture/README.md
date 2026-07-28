# livekit-capture

Capture sources and helpers for publishing video with the LiveKit Rust SDK.
The optional `gstreamer` feature turns a GStreamer `appsink` into an encoded
ingest source; the `demo` feature adds a synthetic pixel source for testing.

## Library entry points

- `pixel::PixelVideoSource` and `encoded::EncodedVideoSource` — the
  libwebrtc-free traits a capture backend implements: pixel sources produce
  frames published through the WebRTC encoder, encoded sources produce
  access units published as passthrough. Both traits are object-safe and
  implemented for `Box<dyn ...>`, so sources can be constructed dynamically
  and driven through the same pumps. Each kind module also holds that kind's
  vocabulary (`pixel::PixelVideoFrame`, `encoded::EncodedAccessUnit`, …).
- `pixel::PixelVideoPump<S>` and `encoded::EncodedVideoPump<S>` — bridge a
  source into a publishable RTC track: each builds the matching
  `NativeVideoSource`, derives publish options (`EncodedVideoPump` selects
  the passthrough encoder), and runs the capture loop on a plain thread.
  Encoded pumps forward downstream keyframe and rate-control requests back
  to the source and drop pre-roll deltas until the first keyframe. Both
  spawn into the same `pump::RunningPump`, so an application supervises
  running pumps of either kind uniformly (`stop()`, `join_async()`, stats);
  the `pump` module holds this shared machinery.
- `sources::gstreamer::ensure_encoded_appsink` and friends turn an arbitrary
  pipeline (containing `appsink name=lk_appsink` or one unlinked encoded pad)
  into an encoded source; `encoded_caps_string` is the single per-codec caps
  table. The GStreamer source answers keyframe requests with a
  `GstForceKeyUnit` upstream event.

## GStreamer ingest

`GStreamerVideoSource` implements `EncodedVideoSource` on top of an
`appsink` producing H.264 (Annex-B or AVC), H.265 Annex-B, VP8, VP9, or AV1
access units. Drive it with an `EncodedVideoPump`, which builds the encoded
RTC source, derives the passthrough publish options, and forwards keyframe
and rate-control requests back to the pipeline. Passthrough is single-layer
(`L1T1`); access units carrying other layering metadata are rejected.
