---
livekit-ffi: patch
---

Creating an audio stream with a noise-cancellation filter no longer blocks the request while the filter loads its model, and both track and participant streams now create the filter session on the blocking thread pool.
