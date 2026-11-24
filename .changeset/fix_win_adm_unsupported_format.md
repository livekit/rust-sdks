---
webrtc-sys: patch
---

Patch the Windows audio device module to fall back to 48 kHz with `AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM` when an endpoint accepts none of its 16-bit formats, instead of failing playout or recording with `AUDCLNT_E_UNSUPPORTED_FORMAT`.
