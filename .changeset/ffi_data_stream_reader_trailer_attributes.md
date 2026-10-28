---
livekit-ffi: patch
---

The `info()` of a `core-modules` data stream reader now includes the attributes from the stream's trailer, which the reader used to drop. They are present once the read that ends the stream returns.
