---
livekit-api: major
---

Upgrade to the breaking `livekit-protocol` release that moves the submodule to `538dd3f0`: `livekit-api`'s own method signatures are unchanged and its two request literals fill the new `UpdateStreamRequest` and `TransferSipParticipantRequest` fields with defaults, but the protocol types it exposes publicly change with it (for example `EgressOutput::Stream(proto::StreamOutput)` now carries `destinations` and `StreamOutput` no longer derives `Eq` and `Hash`), so callers that destructure or construct those payloads, or that pass values from an older `livekit-protocol`, need to update.
