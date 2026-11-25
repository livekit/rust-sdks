# MoQ file changes in the LiveKit Rust SDK

This is the client-side review companion to the [SDK MoQ integration guide](moq.md). The baseline is SDK commit `eb74c3d7`. Paths in the main tables are relative to the SDK workspace root. The matching SFU inventory is `docs/moq-file-changes.md` in the server checkout.

For the shortest path from API to wire, read `room/mod.rs` for transport selection, `rtc_session.rs` for attachment lifetime and `rtc_engine/moq.rs` for protocol messages. The media worker uses the standalone codec APIs, and the session drains the per-publication DataTrack queue. The appendix lists the two protocol schemas and their generated Go bindings.

## Public API, build and documentation

| File | Change | What changed |
| --- | --- | --- |
| [README.md](../../README.md) | Modified | Links to the Rust MoQ integration guide from the workspace’s real-time SDK section. |
| [livekit/README.md](../../livekit/README.md) | Modified | Adds a native-client quickstart with Cargo features, a `RoomTransport::Moq` example, DataTrack usage and links to the implementation guide and room tests. |
| [livekit/docs/moq.md](../../livekit/docs/moq.md) | New | Traces the WebSocket join and QUIC attachment, identifies each library’s responsibility and follows video and DataTrack frames through the SDK. Includes diagrams, typed Rust examples, failure behavior, resource bounds and e2e commands. Links to this inventory. |
| [livekit/docs/moq-feature-parity.md](moq-feature-parity.md) | New | Compares each public Rust room event and the app-facing transport features with default WebRTC, including live observations from all four transport pairs. |
| [livekit/docs/moq-file-changes.md](../../livekit/docs/moq-file-changes.md) | New | Records the SDK worktree delta and the separate dependency revision comparison. This is the file you are reading. |
| [.changeset/moq-rooms.md](../../.changeset/moq-rooms.md) | New | Requests minor releases for `livekit`, `livekit-protocol`, `livekit-signaling`, `libwebrtc` and `webrtc-sys`. Describes the opt-in transport, supported codecs, DataTracks and live route changes. |
| [livekit/Cargo.toml](../../livekit/Cargo.toml) | Modified | Adds the optional `moq` feature. It enables native signaling and standalone codecs, pins moqtap-client 0.16.0 and moqtap-codec 0.13.1 to draft 23, and selects Quinn, rustls, platform certificate verification and URL parsing. Adds the feature to the curated build matrix. |
| [Cargo.lock](../../Cargo.lock) | Modified | Resolves the new MoQ crates and their dependencies. Moves Quinn from 0.11.11 to 0.11.12, updates quinn-proto and records dependencies such as `fastbloom`. The lockfile applies to the SDK workspace. |
| [livekit/src/prelude.rs](../../livekit/src/prelude.rs) | Modified | Re-exports `RoomTransport` beside `Room` and `RoomOptions`, so applications can select the transport using their usual prelude imports. |

## Protocol, signaling and room state

| File | Change | What changed |
| --- | --- | --- |
| [livekit-protocol/protocol](../../livekit-protocol/protocol) | Updated pin | Updates the staged Git submodule pointer from `c6a16489d6aa` to `a609e7a53312`, the protocol revision the SFU also consumes. The appendix lists all four nested file changes. |
| [livekit-protocol/src/livekit.rs](../../livekit-protocol/src/livekit.rs) | Modified | Updates generated Prost bindings for `MoQTransportInfo`, optional `moq` fields in join and reconnect responses, `UnpublishTrackRequest`, and the inter-node `StartSession.use_moq_transport` flag. The attachment message carries the endpoint, ticket, profile, participant SID, generation, expiry and namespace prefix. |
| [livekit-protocol/src/livekit.serde.rs](../../livekit-protocol/src/livekit.serde.rs) | Modified | Updates generated JSON serialization and deserialization for the new messages and fields, including `SignalRequest`’s unpublish variant and the MoQ attachment in join and reconnect responses. |
| [livekit-signaling/src/lib.rs](../../livekit-signaling/src/lib.rs) | Modified | Adds a default-off `use_moq` signal option. Selects the `/rtc` join and appends `transport=moq`, filters the RTP packet-trailer capability when building the protobuf join parameters, and removes the full join response from the signal client's debug representation. |
| [livekit/src/room/mod.rs](../../livekit/src/room/mod.rs) | Modified | Adds `RoomTransport` and `RoomOptions.transport`, defaulting to WebRTC. MoQ selects WebSocket signaling with no early publisher offer, rejects room E2EE, and omits SDP from its sync state. Reconciles early catalog entries when publication metadata arrives over signaling, and clears pending tracks on withdrawal, restart and teardown. |
| [livekit/src/room/participant/local_participant.rs](../../livekit/src/room/participant/local_participant.rs) | Modified | Branches after LiveKit assigns the track SID. MoQ publishes the encoded track, cleans up the pending publication on failure, and explicitly signals unpublish. Disables simulcast and RED for this path and rejects incompatible E2EE or RTP packet-trailer options. |
| [livekit/src/room/track/mod.rs](../../livekit/src/room/track/mod.rs) | Modified | Adds the MoQ-specific subscription failure reason carried by the existing public room event. |
| [livekit/src/room/participant/remote_participant.rs](../../livekit/src/room/participant/remote_participant.rs) | Modified | Allows the subscribed track’s RTP transceiver to be optional. Attaches and detaches decoded MoQ tracks under the same publication lock used by teardown, emitting normal subscription events. |
| [livekit/src/room/e2ee/manager.rs](../../livekit/src/room/e2ee/manager.rs) | Modified | Handles a missing transceiver before accessing its sender or receiver to install a frame cryptor. This protects the shared track lifecycle when it handles tracks produced by the MoQ decoder. |

## Transport session, media and DataTracks

| File | Change | What changed |
| --- | --- | --- |
| [livekit/src/rtc_engine/mod.rs](../../livekit/src/rtc_engine/mod.rs) | Modified | Feature-gates the MoQ module, forwards `MoqMedia` additions and withdrawals with their participant and track SIDs, and adds a test-only `MoqNetworkHandoff` simulation. |
| [livekit/src/rtc_engine/rtc_session.rs](../../livekit/src/rtc_engine/rtc_session.rs) | Modified | Attaches MoQ after signaling joins, closes signaling if attachment fails, and runs the MoQ task with the existing DataTrack queue. Wires publish, unpublish, grant changes and test handoffs into the task. Bypasses WebRTC negotiation and readiness checks, verifies the MoQ task survives signaling resume, rejects legacy data sends and removes credential-bearing join/reconnect response dumps. MoQ keeps the idle publisher object exposed by the existing RTC API. Only WebRTC creates SCTP channels, starts their event tasks or creates a subscriber peer connection. |
| [livekit/src/rtc_engine/moq.rs](../../livekit/src/rtc_engine/moq.rs) | New | Implements the room’s draft-23 session. Validates the attachment, dials native TLS/QUIC, passes SETUP parameters to moqtap, and owns publish/subscribe requests and aliases. Keeps protocol request handles with their workers and explicitly cancels them in the moqtap endpoint on removal. Reconciles the media catalog, isolates per-rendition failures, routes subgroup objects, sends DataTrack packets as datagrams, reassembles complete groups per alias, bounds work and requests a full reconnect after a fatal session failure. |
| [livekit/src/rtc_engine/moq/media.rs](../../livekit/src/rtc_engine/moq/media.rs) | New | Maps catalog codecs to standalone video or Opus workers. Writes LOC samples with timestamps, opens a video group at each keyframe and a group per Opus packet, and decodes incoming objects into native tracks. Checks ordering and video dependencies before delivering decoded frames. |
| [livekit/src/rtc_engine/moq/network.rs](../../livekit/src/rtc_engine/moq/network.rs) | New | Polls the OS route to the connected SFU every 500 ms and rebinds the room’s Quinn endpoint when the selected source changes or a route returns. Keeps the old socket during gaps or bind failures. Includes a test hook and unit tests for detection, retry and recovery. |
| [livekit/src/rtc_engine/dc_sender.rs](../../livekit/src/rtc_engine/dc_sender.rs) | Modified | Constructs the whole-frame queue independently of SCTP. Both transports use the same one-frame pending capacity. The MoQ DataTrack writer reuses the whole-frame queue and can clear pending packets when permissions change. |

## Standalone Rust codec APIs

| File | Change | What changed |
| --- | --- | --- |
| [libwebrtc/Cargo.toml](../../libwebrtc/Cargo.toml) | Modified | Adds `standalone-codecs`, forwarding it to `webrtc-sys` so native codec bindings are built only when selected. |
| [libwebrtc/src/lib.rs](../../libwebrtc/src/lib.rs) | Modified | Exports the standalone audio and video codec modules through the public native API when the feature is enabled. |
| [libwebrtc/src/native/mod.rs](../../libwebrtc/src/native/mod.rs) | Modified | Declares the feature-gated native `audio_codec` and `video_codec` modules used by the public exports. |
| [libwebrtc/src/native/audio_codec.rs](../../libwebrtc/src/native/audio_codec.rs) | New | Adds Rust-owned standalone Opus encoder and decoder types. Accepts PCM frames, produces packets or decoded PCM, and exposes bitrate, DTX, FEC/loss settings and concealment through the native codec bridge. |
| [libwebrtc/src/native/video_codec.rs](../../libwebrtc/src/native/video_codec.rs) | New | Adds safe standalone video encoder/decoder types with configurable backend and rates, keyframe requests, bounded callback delivery and capture timestamp tracking. Includes lifecycle coverage for native codec cleanup, AV1 temporal delimiter handling and bounds on pending frame metadata. |

## Rust/C++ bindings and native codec implementation

| File | Change | What changed |
| --- | --- | --- |
| [webrtc-sys/Cargo.toml](../../webrtc-sys/Cargo.toml) | Modified | Defines the `standalone-codecs` build feature consumed by the higher-level `libwebrtc` crate. |
| [webrtc-sys/build.rs](../../webrtc-sys/build.rs) | Modified | Registers the two additional Rust `cxx` bridge sources and compiles their two C++ implementations when `CARGO_FEATURE_STANDALONE_CODECS` is set. |
| [webrtc-sys/src/lib.rs](../../webrtc-sys/src/lib.rs) | Modified | Exports the new audio and video FFI modules under the same feature gate. |
| [webrtc-sys/include/livekit/audio_codec.h](../../webrtc-sys/include/livekit/audio_codec.h) | New | Declares the native Opus encoder/decoder owners, construction, packet and PCM access, bitrate controls, decode and concealment methods. Specifies the input and output format expected across the bridge. |
| [webrtc-sys/src/audio_codec.rs](../../webrtc-sys/src/audio_codec.rs) | New | Defines the Rust `cxx` interface for Opus settings, encode results, native constructors, borrowed output buffers, loss controls and decoder recovery. |
| [webrtc-sys/src/audio_codec.cpp](../../webrtc-sys/src/audio_codec.cpp) | New | Constructs Opus codecs from libwebrtc’s built-in factories. Converts sample rates and channel layouts, encodes 10 ms PCM inputs, measures RFC 6464 levels across each output packet, decodes packets and supports in-band FEC or packet-loss concealment. |
| [webrtc-sys/include/livekit/video_codec.h](../../webrtc-sys/include/livekit/video_codec.h) | New | Declares standalone native encoder and decoder classes, their callback and task-queue lifetimes, plus encoded-buffer access, H.264 keyframe rewriting and a live-instance counter for teardown tests. |
| [webrtc-sys/src/video_codec.rs](../../webrtc-sys/src/video_codec.rs) | New | Defines the `cxx` types and calls for encoder settings, per-frame information, codec status and operations. Rust observer adapters receive encoded bytes or decoded frame ownership from native callbacks. |
| [webrtc-sys/src/video_codec.cpp](../../webrtc-sys/src/video_codec.cpp) | New | Implements native factory selection and queue-bound codec initialization, encode/decode and teardown. Manages rate and keyframe requests, callback backpressure, frame timing, decoder recovery and helper functions for encoded frames. |
| [webrtc-sys/include/livekit/video_encoder_factory.h](../../webrtc-sys/include/livekit/video_encoder_factory.h) | Modified | Declares `WithVideoEncoderBackend`, which marks a video format with the backend preference used when creating a standalone encoder. |
| [webrtc-sys/src/video_encoder_factory.cpp](../../webrtc-sys/src/video_encoder_factory.cpp) | Modified | Implements `WithVideoEncoderBackend` by reusing the factory’s existing format-tagging helper. Standalone and peer-connection encoder selection therefore use the same backend convention. |

## Room integration tests

| File | Change | What changed |
| --- | --- | --- |
| [livekit/src/room/lifecycle_tests.rs](../src/room/lifecycle_tests.rs) | Modified | Reorders catalog and signaling events, cancels a pending track, and verifies exactly one subscription and unsubscription for a published track. |
| [livekit/tests/common/e2e/video.rs](../../livekit/tests/common/e2e/video.rs) | Modified | Extends the solid-color source fixture to alternate luma on successive captured frames. The default preserves a single color. The room matrix uses alternating frames to verify that the remote decoder continues to advance. |
| [livekit/tests/common/e2e/audio.rs](../../livekit/tests/common/e2e/audio.rs) | Modified | Lets the integration suite identify its generated tone as a microphone for SFU speaker detection. |
| [livekit/tests/moq_test.rs](../../livekit/tests/moq_test.rs) | New | Includes a two-track datagram test and live-server scenarios for the mixed/native transport matrix, full reconnect, permission changes, an independent moqtap subscriber and a live network handoff. Verifies that a live subscriber loses video on revocation and decodes fresh video on regrant. Checks first-subscriber notification before video capture, remote connection quality, active speakers, recovery after a configured subscription limit, decoded video for all five codecs, Opus, 128 KiB fragmented DataTrack payloads, namespace/permission handling, and continuity of the original subscriptions on a handoff. |

## Appendix · full protocol submodule delta

The shared protocol commit `a609e7a53312` applies the MoQ changes directly to the SDK’s previous pin, `c6a16489d6aa`. The four changed files are listed below.

### MoQ schema and generated Go bindings

| Path inside the protocol submodule | Delta at selected pin | Effect |
| --- | --- | --- |
| [protobufs/livekit_rtc.proto](../../livekit-protocol/protocol/protobufs/livekit_rtc.proto) | Modified | Adds `MoQTransportInfo` and `UnpublishTrackRequest`. Allocates tags 16 and 5 for the join and reconnect attachments, respectively, and tag 24 for the unpublish request. Marks the attachment token secret for logging. |
| [livekit/livekit_rtc.pb.go](../../livekit-protocol/protocol/livekit/livekit_rtc.pb.go) | Modified | Regenerates the Go signaling messages, accessors, oneof wrapper and descriptors for the new attachment and unpublish fields used by the SFU. |
| [protobufs/livekit_internal.proto](../../livekit-protocol/protocol/protobufs/livekit_internal.proto) | Modified | Adds `StartSession.use_moq_transport` at tag 28, allowing the signaling node to convey the transport choice to the room node. |
| [livekit/livekit_internal.pb.go](../../livekit-protocol/protocol/livekit/livekit_internal.pb.go) | Modified | Regenerates the Go `StartSession` field, getter and descriptor for the new inter-node flag. |

To reproduce the dependency comparison from the SDK checkout, run these commands.

```sh
git -C livekit-protocol/protocol diff --name-status c6a16489d6aa a609e7a53312
git -C livekit-protocol/protocol show --stat a609e7a53312
```
