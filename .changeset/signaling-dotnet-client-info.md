---
livekit-signaling: patch
livekit: patch
livekit-ffi: patch
---

Report the .NET SDK as `ClientInfo.SDK.DOTNET` in the join request. The `dotnet` SDK name fell through to `UNKNOWN` on the single peer connection path.
