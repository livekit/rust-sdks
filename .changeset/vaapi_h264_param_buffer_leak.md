---
webrtc-sys: patch
---

Fix a per-frame memory leak in the VAAPI H.264 encoder: destroy the parameter and packed-header buffers passed to `vaRenderPicture()` once each frame is submitted, instead of relying on the driver to free them.
