---
livekit-ffi: minor
---

Add `close_with_options(reason, attributes)` to the `core-modules` text and byte stream writers, which sends attributes in the stream's trailer and optionally closes it with a reason. This and the other new data stream methods add requirements to the generated Swift protocols and Kotlin interfaces, so a hand-written conformer such as a test mock must implement them, which is why `livekit-ffi` takes a minor bump.
