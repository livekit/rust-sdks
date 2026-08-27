---
libwebrtc: minor
---

# Expose network_type on IceCandidateStats

Chromium's local `RTCIceCandidateStats` carries a non-standard `networkType` field (WiFi,
cellular, ethernet, etc.), but `IceCandidateStats` had no place to put it, so it was silently
dropped during `get_stats()` deserialization. Adds `network_type: Option<String>` to the struct;
non-breaking since it already derives `#[serde(default)]`.
