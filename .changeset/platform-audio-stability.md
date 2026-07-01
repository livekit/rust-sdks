---
livekit: minor
libwebrtc: minor
livekit-ffi: patch
webrtc-sys: patch
---

Fix platform ADM teardown races on macOS and release FFI handles on dispose.

- Clear remaining FFI handles during `FfiServer::dispose` so native resources are released across repeated initialize/shutdown cycles.
- Stop and detach platform/synthetic audio I/O before peer connection factory teardown, preventing `CaptureWorkerThread` from delivering into destroyed transports.
- Close rooms before dropping FFI track handles and stop platform capture when releasing the platform ADM reference.
- Add `LkRuntime::shutdown_audio_io()` and `PeerConnectionFactoryExt::shutdown_audio_io()` for explicit audio I/O shutdown during runtime teardown.
