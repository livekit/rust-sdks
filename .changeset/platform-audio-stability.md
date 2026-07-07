---
livekit: minor
libwebrtc: minor
livekit-ffi: patch
webrtc-sys: patch
---

Fix platform ADM teardown races on macOS and release FFI handles on dispose.

- Clear remaining FFI handles during `FfiServer::dispose` so native resources are released across repeated initialize/shutdown cycles.
- Stop and detach platform/synthetic audio I/O before peer connection factory teardown, preventing `CaptureWorkerThread` from delivering into destroyed transports. Audio I/O is stopped (joining the worker threads) before the audio callback is detached, since `AudioDeviceBuffer` refuses callback changes while media is active.
- Close rooms before dropping FFI track handles and stop platform capture when releasing the platform ADM reference.
- Add `LkRuntime::shutdown_audio_io()` and `PeerConnectionFactoryExt::shutdown_audio_io()` for explicit audio I/O shutdown during runtime teardown.
- Serialize runtime lifecycles: `LkRuntime::instance()` now waits (bounded, 10s) for a previous runtime's teardown to fully complete before constructing a new peer connection factory/ADM. This closes a race where factory/ADM/transport destruction from one initialize/shutdown cycle could overlap the next cycle's startup.
