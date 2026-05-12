---
webrtc-sys: minor
libwebrtc: minor
livekit-capture: minor
---

feat: zero-copy capture-to-encode pipeline for V4L2 M2M H.264 on Linux.

- `webrtc-sys`: add `DmabufVideoFrameBuffer` (`kNative` `VideoFrameBuffer` wrapping a Linux DMABUF fd with multi-plane offsets/strides and a `libyuv`-backed `ToI420()` fallback) and a `new_native_buffer_from_dmabuf` cxx bridge. The V4L2 H.264 encoder wrapper gains `OutputBufferMode { Mmap, UserPtr, Dmabuf }`; the encoder now imports DMABUF frames via `V4L2_MEMORY_DMABUF`, queues USB/I420 buffers via `V4L2_MEMORY_USERPTR` when contiguous, and constructs `EncodedImageBuffer` directly from the CAPTURE mmap (no intermediate `std::vector`). `V4L2H264EncoderImpl` advertises `supports_native_handle = true` and prefers `{kNative, kI420}`.
- `libwebrtc`: expose `NativeBuffer::from_dmabuf` (Linux-only) and a small `DmabufFrameDesc` / `DmabufPlane` / `Fourcc` API for constructing DMABUF-backed native frames from Rust.
- `livekit-capture` (new crate): runtime-agnostic camera capture sources behind a small `Capture` trait, with a `uvc` backend (USB via `nokhwa`, produces `I420`) and a `libcamera` backend (CSI/Pi via `libcamera-rs`, produces DMABUF-backed `Native` frames). A `Publisher` actor drives capture on a dedicated thread, handles pacing, and forwards frames to `NativeVideoSource`.
- `examples/local_video`: refactored to use `livekit-capture` with a new `--source { uvc, libcamera }` flag. UVC remains the default; `--source libcamera` is the zero-copy path on Pi 4B.
