---
livekit-telemetry: minor
---

Take every telemetry name from the protocol registry (`livekit_protocol::semconv`) and send its spelling: `lk.room_sid`, `lk.participant_id`, `lk.track_sid`, `lk.pii.room_name`, `lk.pii.participant_identity`, `lk.pii.remote_participant_identity`, `lk.track_source` with the RTC FFI `TrackSource` names, and `lk.disconnect_reason` with the protobuf `DisconnectReason` names.
