# livekit-capture

Capture sources and helpers for publishing video with the LiveKit Rust SDK.
The optional `gstreamer` feature turns a GStreamer `appsink` into an encoded
ingest source; the `demo` feature adds a synthetic pixel source for testing.

## Library entry points

- `source::PixelVideoSource` and `source::EncodedVideoSource` — the
  libwebrtc-free traits a capture backend implements: pixel sources produce
  frames published through the WebRTC encoder, encoded sources produce
  access units published as passthrough. Both traits are object-safe and
  implemented for `Box<dyn ...>`, so sources can be constructed dynamically
  and driven through the same pumps.
- `pump::PixelPump<S>` and `pump::EncodedPump<S>` — bridge a source into a
  publishable RTC track: each builds the matching `NativeVideoSource`,
  derives publish options (`EncodedPump` selects the passthrough encoder),
  and runs the capture loop on a plain thread. Encoded pumps forward
  downstream keyframe and rate-control requests back to the source and drop
  pre-roll deltas until the first keyframe. Both spawn into the same
  `pump::RunningPump`, so an application supervises running pumps of either
  kind uniformly (`stop()`, `join_async()`, stats).
- `track::NativeVideoSourceExt` — extension methods on the RTC-level
  `NativeVideoSource` for capturing pre-encoded access units. Use
  `NativeVideoSource::new_encoded` for pre-encoded passthrough (no raw
  keepalive frames, so the sender starts directly on the passthrough encoder).
- `EncodedIngress` — the pre-encoded pump used when the caller manages its
  own source: `capture_next()` reports each published access unit,
  `stop_handle()` cancels from any thread, and downstream keyframe requests
  (PLI/FIR) are forwarded to the source automatically. Passthrough is
  single-layer (`L1T1`), and access units carrying other layering metadata are
  rejected.
- `sources::gstreamer::ensure_encoded_appsink` and friends turn an arbitrary
  pipeline (containing `appsink name=lk_appsink` or one unlinked encoded pad)
  into an encoded source; `encoded_caps_string` is the single per-codec caps
  table. The GStreamer source answers keyframe requests with a
  `GstForceKeyUnit` upstream event.

## GStreamer ingest

`GStreamerAppSinkEncodedSource` implements `EncodedAccessUnitSource` on top of
an `appsink` producing H.264 (Annex-B or AVC), H.265 Annex-B, VP8, VP9, or
AV1 access units. Feed it to `EncodedIngress` together with a
`NativeVideoSource::new_encoded` RTC source, then publish a local video track
created from that source with `track::encoded_publish_options(codec)` so the
sender uses the pre-encoded passthrough encoder.
