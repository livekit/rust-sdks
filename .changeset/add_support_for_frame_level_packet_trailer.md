---
livekit: minor
livekit-protocol: minor
livekit-api: minor
webrtc-sys: minor
livekit-ffi: minor
libwebrtc: minor
---

# Add support for frame level packet trailer

#890 by @chenosaurus

- Add support to attach/parse frame level timestamps & frame ID to VideoTracks as a custom payload trailer.