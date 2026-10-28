---
livekit-signaling: minor
---

Add `SignalOptions::region_failover` so a caller that runs its own region failover can turn off the retry on other LiveKit Cloud regions in `SignalClient::connect`.
