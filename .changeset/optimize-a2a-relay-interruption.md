---
livekit-a2a-relay: patch
---

Optimise the LiveKit A2A Relay's turn-taking and speech interruption loops. Implement onset-smoothed VAD tracking to ensure interruptions only trigger on sustained user speech rather than transient noise or echo spikes. This eliminates false-positive playback cuts and ensures smooth audio output.
