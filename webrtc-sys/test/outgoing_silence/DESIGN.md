# Conservative outgoing zero-input scheduling

This implementation coalesces encoder-queue submissions within the next packet
boundary. It does not implement long DTX sleeps or remove encoder computation.

The buffered source continues emitting every 10 ms. `ChannelSend` assigns RTP
timestamps as before, classifies exact-zero PCM, and hands the original owned
`AudioFrame` to `SilenceEncoderTaskQueue`. No sample, capture metadata or media
time is invented or discarded.

`AudioEncoder::Num10MsFramesUntilSilenceWork()` is an internal native capability.
Zero means unsupported; a positive N permits holding at most N-1 upcoming zero
frames. Opus reports the remaining frames in its current packet. Opus with ANA
or 10-ms packets returns unsupported. RED delegates to its inner encoder; other
codecs/wrappers default to unsupported. This requires a matching native rebuild.

The queue deliberately caps deferral at one frame, regardless of a larger codec
budget. A second zero frame, nonzero input or a control posts the pending frame
first. The normal encoder consumes every frame in the original order. Completed
packets, including DTX-empty callbacks, therefore retain their natural deadline.
This uses actual codec packet state rather than an oracle or DTX timer guess.

Admission of media and controls shares a mutex. Controls that must complete
synchronously use a queue barrier, flushing earlier pending input before the
configuration changes. Mute, encoder replacement, bitrate/loss controls, CSRCs,
audio-level configuration, frame encryption and frame transformation retain
that ordering. Calls already on the encoder queue execute synchronously.

A generation counter prevents an older in-flight task from publishing a work
budget over newer queued work. Unknown/stale state falls back to immediate
submission. Unsupported capability is cached until a control invalidates it.
Nonzero input does not perform the extra codec-capability query.

The source clock, its completion/backpressure callbacks, upstream processing,
resamplers, RTP timestamp assignment and independent RTP/RTCP timers are
unchanged. Audio levels and bitrate accounting still process each original
frame. Native queue-latency telemetry measures time after the batch is posted,
excluding intentional holding before a packet boundary.

## Ownership

`ChannelSend` owns the coalescer and the existing encoder task queue. The
coalescer borrows the queue and clock; its handlers borrow `ChannelSend` through
raw non-owning captures. The existing sender destructor deletes/joins the task
queue before member destruction, while the coalescer remains alive. Queued
callbacks cannot execute after that join. Pending storage outside the existing
native task queue is bounded to one owned frame; posted batches hold at most two.
No new task queue, thread, timer, FFI handle or strong ownership cycle is added.

## Remaining gates

Keep this as a draft until a complete native build passes on the target platform
and representative CPU measurements establish a benefit. Mac results have not
established a CPU saving. More aggressive DTX deferral still needs an independent
codec-owned deadline/state argument; this patch does not supply one. It does not
fix unrelated single-PC codec renegotiation behavior or unbuffered-source gaps.
