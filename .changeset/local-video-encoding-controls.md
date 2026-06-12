---
libwebrtc: patch
livekit: patch
---

Add simulcast-aware runtime video encoding limit controls for local video tracks, preserving the publish-time simulcast ladder while applying track-level caps through the `local_video` publisher/subscriber example RPC.
