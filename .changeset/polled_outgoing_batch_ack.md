---
livekit-ffi: major
---

The `core-modules` polled outgoing data stream adapter now waits for the host to report each batch's outcome. `OutgoingPacketQueue.next_packets()` is replaced by `next_batch()`, which returns an `OutgoingPacketBatch` with `packets()`, `complete()` and `fail(reason)`. A `send_*`/`write` call resolves only once its batch is completed, and fails when the batch fails, is released without an outcome, or the queue is closed. Before, the adapter acknowledged sends as soon as the packets were buffered, so send errors were lost and producers had no backpressure.
