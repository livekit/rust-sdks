---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Ingest length-prefixed (`avc3`/`hev1`) pre-encoded H.264 and H.265 with `LengthPrefixedAccessUnitParser`, `access_unit_from_h265_hvcc` and `hev1` caps in the GStreamer source, read LKTS packet-trailer SEI with `frame_metadata_from_sei`, and announce the codec of single-layer video tracks to the server.
