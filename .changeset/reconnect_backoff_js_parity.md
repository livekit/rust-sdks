---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Space reconnect attempts on the same schedule as livekit-client's `DefaultReconnectPolicy` (0.3 s, 1.2 s, 2.7 s, 4.8 s, then 7 s, plus up to 1 s of jitter) instead of full jitter, which could spend all ten attempts within a few seconds of outage. The jitter now comes from a randomly keyed hash rather than the clock's sub-second digits, which are all zero on macOS. No new reconnect attempt starts after `RECONNECT_MAX_DURATION` (60 s), so slow failing attempts cannot keep a room in `Reconnecting` for minutes. `RECONNECT_BACKOFF_MULTIPLIER` is deprecated: the schedule is quadratic in the attempt number.
