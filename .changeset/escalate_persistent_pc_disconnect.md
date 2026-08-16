---
livekit: patch
---

Escalate a PeerConnection that stays disconnected, instead of waiting for `Failed`.

Only `PeerConnectionState::Failed` triggered recovery, and libwebrtc does not reach it until
ICE consent expires — tens of seconds after the transport actually stopped working. A session
whose media plane died stayed "connected" and silently deaf for that entire window before
anything began recovering. A transport entering `Disconnected` now starts a grace period, and
is treated as failed if it has not recovered when that elapses; brief disconnects during
ordinary network disturbance still resolve on their own and are ignored.
