---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
---

Escape XOR'd H.264 packet-trailer TLVs with Annex-B emulation-prevention bytes so a trailer containing `FF FF FE` is not split into a NAL start code.
