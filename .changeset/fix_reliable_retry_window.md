---
livekit: patch
livekit-ffi: patch
---

# Keep a resume-replay window in the reliable retry queue

The reliable retry queue was trimmed with the flushed byte count as the
target size to keep, so it retained roughly the last flushed burst
(typically one packet) and a resume could replay almost none of the
reliable packets lost around a reconnect. The queue is now trimmed to the
flushed bytes plus a floor of 1.25x the buffered-amount low threshold, so
the retained window covers the full backpressure amount.
