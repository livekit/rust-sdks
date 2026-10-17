---
"livekit-signaling": patch
---
Retain failed and unattempted queueable signals in FIFO order until a transport acknowledges them, without replaying a successfully written prefix after reconnecting.
