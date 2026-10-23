---
livekit-signaling: minor
---

Add `SignalOptions::protocol_version` and `SignalOptions::client_capabilities` so a caller whose media stack lags behind the Rust SDK can advertise its own protocol version and capabilities to the server.
