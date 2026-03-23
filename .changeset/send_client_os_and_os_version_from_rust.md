---
livekit: patch
livekit-api: patch
livekit-ffi: patch
---

# Send client os and os_version from rust

#952 by @MaxHeimbrock

Adds [os_info](https://crates.io/crates/os_info) crate as dependency and sends the data for client connections.
