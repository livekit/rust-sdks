---
livekit: patch
---

Space reconnect attempts on the same schedule as livekit-client's `DefaultReconnectPolicy` (0.3 s, 1.2 s, 2.7 s, 4.8 s, then 7 s, plus up to 1 s of jitter) instead of full jitter, which could spend all ten attempts within a few seconds of outage.
