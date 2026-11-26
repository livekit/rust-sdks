---
livekit-ffi: minor
livekit: minor
---

Keep room shutdown owned after a bounded disconnect wait, release media on cancellation, preserve late room identifiers and terminal event ordering, and remove cancelled pending track publications. The low-level `RtcEngine::connect` returns guarded `PendingTrack` futures for startup publication responses instead of raw oneshot receivers. Disconnect now reports a typed shutdown timeout when cleanup is still running.
