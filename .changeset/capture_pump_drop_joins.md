---
livekit-capture: minor
---

Dropping a `RunningPump` now stops the pump and waits up to 5 seconds for its thread to exit, so the capture source is released by the time the drop returns; the same applies to a cancelled `RunningPump::join`, `RunningPump::finished` waits for the thread to exit without consuming the handle, and `PumpError` is now `#[non_exhaustive]`.
