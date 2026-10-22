---
livekit-ffi: minor
---

Add `close_with_options(reason, attributes)` to the `core-modules` text and byte stream writers, which sends attributes in the stream's trailer and optionally closes it with a reason.
