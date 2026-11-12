---
livekit-ffi: patch
---

Expose the data stream readers and writers (`ByteStreamReader`, `TextStreamReader`, `ByteStreamWriter`, `TextStreamWriter`) over UniFFI in room-apis builds too, as the same objects core-modules exports. A reader or writer handle from the C ABI (`OwnedByteStreamReader`, `OwnedTextStreamWriter`, ...) can be taken up with `from_ffi_handle_id` and handed back with `ffi_handle_id`, as with data tracks; the protobuf request surface is unchanged.
