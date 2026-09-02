# LiveKit Region

An internal crate holding the region-discovery cache shared by the LiveKit
signalling client and the server-API failover path: the `/settings/regions`
response types, the cloud-host and `Cache-Control` helpers, and `RegionCache`.

To build applications with LiveKit, please use the public APIs provided by the
[livekit](../livekit) and [livekit-api](../livekit-api) crates.
