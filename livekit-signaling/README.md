# LiveKit Signaling

An internal crate holding the LiveKit signalling client: the WebSocket signal
stream, reconnect/resume handling, and LiveKit Cloud region discovery. Transport
is provided by [livekit-net](../livekit-net), so this crate is blind to the
backend.

To build applications with LiveKit, please use the public APIs provided by the
[livekit](../livekit) crate.
