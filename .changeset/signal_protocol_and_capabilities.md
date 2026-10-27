---
livekit-signaling: minor
---

Add `SignalOptions::protocol_version`, `SignalOptions::client_capabilities` and `SignalOptions::client_protocol` so a caller whose media stack lags behind the Rust SDK can advertise its own protocol versions and capabilities.
