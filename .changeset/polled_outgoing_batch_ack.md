---
livekit-ffi: major
---

The polled outgoing data stream adapter resolves a send once the host reports its batch as sent, and no longer when the packets are buffered. `OutgoingPacketQueue.next_packets()` is replaced by `next_batch()`, which returns the packets and an `ack` the host must `complete()` or `fail(reason)`.
