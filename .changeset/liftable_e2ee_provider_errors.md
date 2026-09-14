---
livekit-datatrack: major
livekit-uniffi: major
livekit: patch
livekit-ffi: patch
livekit-capture: patch
---

`EncryptionError::Failed` and `DecryptionError::Failed` carry a `reason` string and are no longer `flat_error`, so a foreign `EncryptionProvider` or `DecryptionProvider` returning an error no longer aborts the process with "Can't lift flat errors" -- a failed data track decrypt (no E2EE manager, key mismatch, corrupt frame) now drops the frame and leaves the room connected.
