---
livekit-protocol: minor
---

Add `semconv`, the telemetry names and enum values generated from the protocol's Weaver registry, and move the protocol submodule to `538dd3f0`, the merged registry. This is a breaking upgrade of the generated Rust source, taken as a minor bump before 1.0 with no compatibility alias: `DataTrackExtensionId::DteiParticipantSidLow` is gone and value 1 is now `DteiE2ee` (JSON name `DTEI_E2EE`, so the old name no longer parses), `media_source::Video` gains the `Videos` variant, `media_source::Video`, `StreamOutput`, `UpdateStreamRequest` and `MigrateJobRequest` lose `Eq` and `Hash`, `simulation_run::job_metrics::Turn` loses `Copy`, and request messages such as `UpdateStreamRequest` and `TransferSipParticipantRequest` gain fields, so exhaustive matches and struct literals in downstream code need `..Default::default()` or new arms.
