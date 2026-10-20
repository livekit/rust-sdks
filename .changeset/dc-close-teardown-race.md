---
livekit: patch
---

Stop reporting an expected publisher data channel close as unexpected. A server-side teardown (room deleted, participant removed) closes the channels from the remote side before its `Leave` is processed, so every such disconnect logged `publisher data channel '_reliable' closed unexpectedly` at ERROR, three times over. The close is now classified once the teardown that explains it has had time to arrive; a close nothing explains is still reported. Reconnect progress (`resuming connection...`, `restarting connection...`) drops from ERROR to INFO, so it no longer reads as a failure.
