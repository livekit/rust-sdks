---
livekit-data-stream: patch
---

`ByteStreamReader::write_to_file` no longer overwrites an existing file or follows a symlink at the destination. The file name can come from the remote sender, so the destination is now opened with `create_new`, and an existing path returns `StreamError::Io` with `AlreadyExists` instead of being truncated.
