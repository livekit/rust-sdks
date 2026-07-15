# livekit-capture

`livekit-capture` provides shared capture types and publishing helpers for the
LiveKit Rust SDK. It supports decoded frames, native buffers, Linux DMA-BUF
frames, and pre-encoded video access units.

Concrete capture sources are optional and are introduced behind crate features.

## Core entry points

- `VideoCaptureTrack` creates a publishable LiveKit video track and accepts
  decoded, DMA-BUF, or pre-encoded frames.
- `CaptureFrameSource` is the common interface implemented by capture sources.
- `CaptureFrame` represents native, raw, DMA-BUF, or encoded output.
- `EncodedIngress` drives an encoded source while forwarding keyframe and
  rate-control requests upstream.

The encoded path accepts H.264, H.265, VP8, VP9, and AV1 access units. The
shared helpers include Annex-B and AVC parsing plus RTP depacketization used by
network capture sources.
