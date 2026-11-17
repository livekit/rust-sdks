---
livekit-capture: patch
---

Treat H.265 CRA and BLA pictures, not only IDR, as keyframes when they carry VPS/SPS/PPS, so open-GOP streams such as x265's default output publish pre-encoded.
