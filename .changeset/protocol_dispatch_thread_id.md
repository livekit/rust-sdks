---
livekit-protocol: patch
livekit-api: patch
---

Bump the protocol to livekit/protocol@b499314: `thread_id` on `CreateAgentDispatchRequest`,
`RoomAgentDispatch`, `AgentDispatch` and `Job` (livekit/protocol#1958), plus the protocol changes
since the previous pin. `livekit-api` fills the new `UpdateStreamRequest.add_destinations` and
`TransferSipParticipantRequest.dialtone` fields with their defaults.
