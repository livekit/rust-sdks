---
livekit-hwcodec: patch
livekit-bitstream: patch
---

Makes the V4L2 encoder in `livekit-hwcodec` check that its output is usable and fail the session, with a report, when the driver stalls, errors or produces an undecodable stream.
