---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Fix a local publication and its track leaking when unpublish cannot reach the transport.

Unpublishing asks the engine to remove the RTP sender and stopped on failure, skipping the
cleanup that detaches the track from its publication. The publication holds its track and
the track holds the publication back through its mute callbacks, so the pair kept itself
alive along with the transceiver and its peer connection. Removing the sender fails on two
routine paths — an abnormal disconnect, where the transport is already closed, and a full
reconnect, where the sender belongs to the replaced transport — and the error was discarded,
so neither surfaced. The local cleanup now always runs and the failure is reported once
local state is consistent.
