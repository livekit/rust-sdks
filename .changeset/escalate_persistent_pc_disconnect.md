---
livekit: patch
livekit-ffi: patch
---

Escalate a PeerConnection that stays disconnected, instead of waiting for `Failed`.

Only `PeerConnectionState::Failed` triggered recovery, and libwebrtc does not reach it until
ICE consent expires — tens of seconds after the transport actually stopped working. A session
whose media plane died stayed "connected" and silently deaf for that entire window before
anything began recovering. A transport entering `Disconnected` now starts a grace period, and
is treated as failed if it has not recovered when that elapses; brief disconnects during
ordinary network disturbance still resolve on their own and are ignored.

A transport that is *repairing* when the window ends — `Connecting`, meaning an ICE restart is
under way — is given one further window rather than escalated, since gathering, connectivity
checks and DTLS can exceed a single window on a relayed or lossy path. And when a reconnect is
already in flight, the countdown stands down entirely: that reconnect judges the same transport
against transition counts rather than a single state read, and on its own deadline, so a slow
resume is no longer converted into a full track-republishing rebuild.
