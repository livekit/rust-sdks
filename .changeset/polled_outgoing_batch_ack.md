---
livekit-ffi: major
---

The polled outgoing data stream adapter resolves a send once the host reports its batch as sent, and no longer when the packets are buffered. `OutgoingPacketQueue.next_packets()` is replaced by `next_batch()`, whose `OutgoingPacketBatch` the host must `complete()` or `fail(reason)`.
