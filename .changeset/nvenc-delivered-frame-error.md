---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
livekit-capture: patch
---

Stop the NVIDIA and Jetson encoders from reporting an already delivered frame as an encode error, which aborted the process in `SimulcastEncoderAdapter` when dynacast paused a simulcast layer or a track was unpublished mid-encode.
