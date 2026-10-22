---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
webrtc-sys-build: patch
---

fix: bump libwebrtc to webrtc-a545de4 to drop roots that trust the compromised eMudhra intermediates. The new build refreshes the TURN/TLS anchors compiled into `rtc_base/ssl_roots.h`, which no longer include `AAA Certificate Services`, the root that cross-signs the eight eMudhra intermediate CAs whose keys were disclosed on 2026-10-20.
