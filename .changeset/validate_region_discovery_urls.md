---
livekit-api: patch
livekit-region: patch
livekit-signaling: patch
---

Validate region URLs returned by `/settings/regions` before connecting to them. Region discovery and API failover previously validated only the configured host and used the URLs from the discovery response as-is; each returned URL is now independently required to name a LiveKit Cloud host and use an encrypted scheme (`wss`/`https`).
