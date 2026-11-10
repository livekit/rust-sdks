---
livekit-hwcodec: patch
---

Adds the V4L2 encoder to `livekit-hwcodec`, which runs each session on its own worker thread so that no caller waits on the driver.
