---
livekit: minor
libwebrtc: minor
livekit-ffi: patch
webrtc-sys: patch
---

Fix platform ADM teardown races on macOS and release FFI handles on dispose.

- Clear remaining FFI handles during `FfiServer::dispose` so native resources are released across repeated initialize/shutdown cycles.
- Stop and detach platform/synthetic audio I/O before audio transports are unregistered and before peer connection factory teardown, preventing `CaptureWorkerThread` from delivering into destroyed transports. Audio I/O is stopped (joining the worker threads) before the audio callback is detached, since `AudioDeviceBuffer` refuses callback changes while media is active.
- Pause and join platform capture while removing audio senders and through room transport teardown, preventing `AudioTransportImpl::SendProcessedData` from dispatching through an `AudioSender` entry that is being destroyed.
- Close rooms before dropping FFI track handles, release closed RTC sessions after snapshotting their stats, and stop platform capture when releasing the platform ADM reference. Runtime-scoped session guards shut down and detach audio I/O before the last session's peer transports are reclaimed.
- Keep reusable platform-ADM release separate from terminal shutdown: stop workers and detach the platform callback on release, restore it on reacquire, and reinitialize retained ADMs when WebRTC terminates an idle audio engine.
- Add `LkRuntime::shutdown_audio_io()` and `PeerConnectionFactoryExt::shutdown_audio_io()` for explicit audio I/O shutdown during runtime teardown.
- Serialize runtime lifecycles: `LkRuntime::instance()` now waits (bounded, 10s) for a previous runtime's teardown to fully complete before constructing a new peer connection factory/ADM. This closes a race where factory/ADM/transport destruction from one initialize/shutdown cycle could overlap the next cycle's startup.
