---
livekit-rpc: patch
livekit: patch
livekit-ffi: patch
livekit-capture: patch
---

Stop failing an RPC call whose reply arrives just after its recipient leaves the room.

Purging a call's pending ack and response the instant its recipient disconnects assumed that
"participant left" implies "no reply is coming". It does not: the disconnect is announced on the
signalling connection while the ack and response travel on the data channel, and nothing orders
the two. A recipient that leaves *because of* the RPC it just answered — `lk.sip.EndCall`, where
the SIP participant drops out of the room as soon as it has ended the call — produces that order
routinely, so the reply was discarded ("Response received for unexpected RPC request") and the
caller was handed `RecipientDisconnected` for a call that in fact succeeded.

A disconnect now shortens the wait instead of ending it: the pending entries stay registered for
up to 2 more seconds, so a reply already in flight still resolves the caller, and a recipient that
really did leave without replying still fails fast rather than running out the 15s response
timeout.
