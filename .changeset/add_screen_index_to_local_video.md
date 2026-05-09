---
local_video: patch
libwebrtc: patch
webrtc-sys: patch
---

Add a `--screen-index` option to the local video publisher for publishing a selected screen instead of a camera, and make publisher source flags mutually exclusive.

Also allow macOS screen source enumeration to fall back to CoreGraphics display IDs when WebRTC does not return screen sources.
