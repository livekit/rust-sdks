---
livekit-ffi: minor
---

The `core-modules` text and byte stream writers' `close` takes an optional `reason`, which closes the stream abnormally, and `attributes` to send in the stream's trailer, which default to none and an empty map. This and the other changed or new data stream methods change the generated Swift protocols and Kotlin interfaces, so a hand-written conformer such as a test mock must be updated.
