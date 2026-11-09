---
livekit-ffi: minor
livekit-telemetry: minor
---

Rename the exported RTC sample field and stats report argument from `track_sid` to `track_id`; Rust and foreign callers must migrate the input name while continuing to pass the same LiveKit track identifier values.
