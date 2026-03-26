---
livekit: minor
livekit-protocol: minor
livekit-api: minor
webrtc-sys: minor
livekit-ffi: minor
libwebrtc: minor
imgproc: no changelog additions
---

# Add support for frame level packet trailer

#890 by @chenosaurus

- Add support to attach/parse frame level timestamps & frame ID to VideoTracks as a custom payload trailer.
- Breaking change in VideoFrame API, must include `frame_metadata` or use VideoFrame::new().