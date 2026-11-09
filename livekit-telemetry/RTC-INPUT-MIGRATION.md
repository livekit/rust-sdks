# RTC input name migration

This is a breaking input API change: `RtcStatsSample.track_sid` is now `track_id` in Rust and generated host records. Update struct literals, field access and named constructor arguments. There is no compatibility alias.

`RtcStatsSample::new(track_id: impl Into<String>, kind: TrackKind, direction: StreamDirection) -> Self` takes the same LiveKit identifier, for example `TR_camera`. The value is not replaced by a WebRTC `trackIdentifier`; peer reports still map those WebRTC identifiers to LiveKit track identifiers.

Report APIs use `track_id` as their argument name in the pipeline and foreign wrappers. Native room/protocol SID types and unrelated span/track APIs retain their existing names. The cumulative registry integration continues to emit only canonical `lk.track_id`, with no legacy/canonical dual emission. The rename itself does not change emitted values or statistics grouping.

No new public batch API is introduced. Separate typed sample calls still require a shared timestamp for one poll and remain individually admitted; existing whole-report APIs provide atomic report admission.
