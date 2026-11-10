---
livekit-net: major
livekit-ffi: major
livekit-signaling: major
livekit-token-source: major
livekit-telemetry: major
livekit: major
livekit-capture: major
livekit-api: patch
---

Add an opt-in decoded HTTP response limit, used by telemetry export, that reports an oversized response as the new `TransportError::ResponseTooLarge` variant. Code that matches `TransportError` exhaustively, in Rust or in the generated Swift and Kotlin bindings, needs a case for it.
