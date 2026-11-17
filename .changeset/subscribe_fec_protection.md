---
livekit: minor
livekit-api: patch
livekit-protocol: minor
---

Add `RemoteTrackPublication::set_fec_protection` with the new `SubscriptionFecProtection` and `SubscriptionFecLevel` to request disabled, adaptive, fixed-level or minimum-level FlexFEC repairs from the SFU for a subscribed video track, using the new `UpdateTrackSettings.fec` protocol field.
