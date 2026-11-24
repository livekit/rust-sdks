---
livekit-data-stream: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
livekit-rpc: patch
---

`ByteStreamReader::write_to_file` no longer overwrites an existing file or follows a symlink at the destination. The file name can come from the remote sender, so the destination is now opened with `create_new`, and an existing path returns `StreamError::Io` with `AlreadyExists` instead of being truncated. A transfer that fails or is cancelled part-way now removes the file it created, so retrying to the same name works.
