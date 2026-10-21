---
livekit-capture: minor
---

Dropping a `RunningPump` now stops the pump and waits up to 5 seconds for its thread to exit, so the capture source is released by the time the drop returns; a cancelled `RunningPump::join` no longer blocks, and `PumpError` is now `#[non_exhaustive]`.
