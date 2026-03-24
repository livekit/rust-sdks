---
livekit: minor
livekit-protocol: minor
livekit-api: minor
livekit-wakeword: no changelog additions
soxr-sys: no changelog additions
webrtc-sys-build: no changelog additions
webrtc-sys: minor
livekit-ffi: minor
yuv-sys: no changelog additions
libwebrtc: minor
imgproc: no changelog additions
---

# Add support for frame level packet trailer

#890 by @chenosaurus

- Add support to attach/parse frame level timestamps & frame ID to VideoTracks as a custom payload trailer.