---
livekit-a2a-relay: patch
---

Fix floor control log spam in `OfficialA2aClient` by changing `info!` to `debug!` for `request_floor` and `release_floor` methods. Additionally, optimize the `a2a_mock_agent` example to immediately handle empty text inputs from VAD silence or background noise, bypassing a 5-second Ollama connection timeout.
