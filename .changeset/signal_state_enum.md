---
livekit-signaling: patch
---

# Model the signal connection lifecycle as an explicit state machine

`SignalInner` tracked its lifecycle in two fields, a stream slot and a `reconnecting` flag,
that had to be kept in step by hand. It now holds one `SignalState` that owns the transport
(`Connected`, `Reconnecting`, `Offline`, `Disconnecting`, `Closed`). Every change goes through one
transition table, and an input a state does not accept is logged and refused. A resume
started from a state that cannot accept it now fails with `SignalError::InvalidState`
instead of proceeding.

The held-signal queue moved to a sync lock that is never held across an await, which removes
a lock-order hazard between the queue and the stream lock. A send that fails for any
transport error is now held like a `SendError` was.
