## 0.2.0 (2026-11-13)

### Breaking Changes

- Add an opt-in decoded HTTP response limit, used by telemetry export, that reports an oversized response as the new `TransportError::ResponseTooLarge` variant. Code that matches `TransportError` exhaustively, in Rust or in the generated Swift and Kotlin bindings, needs a case for it.

### Fixes

- Scope agent database access to a thread or a database: `AgentThreadGrants` gains `sql`, and the new `agentDatabase` claim (`AgentDatabaseGrants`) grants `read`, `write` and `delete` on listed databases plus project-wide `create` and `list`. `AgentGrants::database_admin` is removed, and `DatabaseClient` signs each call with only the grant it needs. A thread's database id is its thread id, so `Thread::database_id` is gone.
- Fix remote participant and track leaks when closing a room, including encrypted tracks, cancelled closure, and participant updates racing with teardown.
- Handle out-of-range key indices gracefully in `KeyProvider` and clamp `key_ring_size` to valid bounds.
- Log libwebrtc messages at their own severity instead of reporting every one as debug, so its warnings and errors are no longer filtered out by default in every SDK.
- On a full reconnect, `Room` now reports `Reconnecting` (and the `ConnectionStateChanged` that precedes it) before removing remote participants, so the `ParticipantDisconnected` events of a full reconnect arrive after `Reconnecting` rather than before it and can be told apart from participants actually leaving. #1591 (@MaxHeimbrock)
- When the local room closes, in-flight `perform_rpc` calls now fail with `RpcErrorCode::ConnectionClosed` (1506) instead of `RecipientDisconnected` (1503), which remains reserved for remote participant departures.
- Report the .NET SDK as `ClientInfo.SDK.DOTNET` in the join request. The `dotnet` SDK name fell through to `UNKNOWN` on the single peer connection path. - #1669 (@MaxHeimbrock)

## 0.1.9 (2026-11-04)

### Fixes

- Pre-encoded video publishing no longer requires a local encoder for its codec, so pre-encoded H265 works on hosts without an H265 encoder, a camera track requesting a codec no local encoder produces now publishes VP8 instead of a codec it cannot encode, and `PeerConnectionFactory::video_encoder_codecs` reports codec support using a non-exhaustive `libwebrtc::video_codec::VideoCodec` enum supporting `FromStr`, with `Auto` excluding the pre-encoded pass-through backend.
- Drop a transcription with an invalid track sid instead of panicking - #1600 (@lukasIO)
- Preserve distinct frame metadata when queued video frames are captured within the same millisecond, including pre-encoded frames that share an RTP timestamp.
- livekit-signaling: end a session for good through a cancellation token - #1589 (@lukasIO)
- livekit-signaling: fix teardown gaps on requested close and response timeout - #1568 (@lukasIO)
- `OfferOptions::offer_to_receive_audio` and `offer_to_receive_video` are now `Option<bool>`. `None` leaves libwebrtc's option unset, so receive-only transceivers stay receive-only. The default is still `Some(false)`. Single peer connection offers leave both unset and no longer rewrite `a=inactive` to `a=recvonly`.
- Space reconnect attempts on the same schedule as livekit-client's `DefaultReconnectPolicy` (0.3 s, 1.2 s, 2.7 s, 4.8 s, then 7 s, plus up to 1 s of jitter) instead of full jitter, which could spend all ten attempts within a few seconds of outage. The jitter now comes from a randomly keyed hash rather than the clock's sub-second digits, which are all zero on macOS. No new reconnect attempt starts after `RECONNECT_MAX_DURATION` (60 s), so slow failing attempts cannot keep a room in `Reconnecting` for minutes. `RECONNECT_BACKOFF_MULTIPLIER` is deprecated: the schedule is quadratic in the attempt number.

## 0.1.8 (2026-10-28)

### Features

- Dropping a `RunningPump` now stops the pump and waits up to 5 seconds for its thread to exit, so the capture source is released by the time the drop returns; the same applies to a cancelled `RunningPump::join`, `RunningPump::finished` waits for the thread to exit without consuming the handle, and `PumpError` is now `#[non_exhaustive]`.
- Create the well-known data channels as negotiated (fixed IDs) when the server supports it (protocol 18), saving the data channel open handshake round trip. Falls back to in-band channels against older servers.

### Fixes

- Keep NVIDIA's Jetson video codec plugin loaded while probing MMAPI encoder capabilities, preventing probe teardown from breaking subsequent GStreamer capture.
- An incoming single-packet (inline) data stream whose payload is shorter or longer than the length in its header now fails with `StreamError::Incomplete` or `StreamError::LengthExceeded`, as a chunked stream already does. Before, it ended successfully.
- Split outgoing data stream options into `SendTextOptions`, shared `SendByteOptions`, `StreamTextOptions` and `StreamByteOptions`, all `#[non_exhaustive]`. Remove `send_file_with_progress` and `stream_text_with_total_length` in favor of `on_progress` and `total_length` on the options.
- Add data stream progress to `core-modules`: readers get `next_progress()`, which counts received bytes, and `send_text`, `send_bytes` and `send_file` take an optional `progress` delegate (`StreamProgressDelegate`) inside their options record, defaulting to none and ignored by incremental streams. The core adds `on_progress` on `SendTextOptions` and `SendByteOptions` and a `progress_stream()` accessor on the concrete readers that does not borrow the reader. The generated Swift option records no longer conform to `Equatable` or `Hashable`.
- Let an incremental text stream declare its total length in the header: the core adds `StreamTextOptions::total_length`, and the `core-modules` `StreamTextOptions` record gets an optional `total_length` that `stream_text` passes to it. The receiver then reports progress against it and fails the stream if the length does not match.
- A data stream that its sender closes early with a reason now fails with that reason (`StreamError::AbnormalEnd`), as in JS, even when its header declared a total length; before, the reader got `StreamError::Incomplete`.
- Release local tracks when the server ends the room by detaching publisher senders before the publisher PeerConnection closes, and make `RtpSender::track()` return `None` for a detached sender instead of dereferencing null.
- Add `close()` to the `core-modules` text and byte stream readers (`closeStream()` in Kotlin) and core `StreamCloser` handles returned by `closer()` to end pending reads with `DataStreamError::AlreadyClosed` and remove streams exactly once; dropping a reader has the same effect.
- Add `InferenceGrants` and `AccessToken::with_inference_grants`, so Rust services can mint tokens for LiveKit Inference.
- livekit-signaling: add an opt-out for region failover in connect - #1567 (@lukasIO)
- livekit-signaling: let callers set the advertised protocol and capabilities - #1569 (@lukasIO)
- Add `ObservabilityGrants` and `AccessToken::with_observability_grants`, so Rust services can mint tokens that publish observability data.
- Remove the `use_legacy_data_streams` migration flag. The FFI `RoomDataStreamOptions.use_legacy_client_implementation` field is now deprecated and ignored, and the hidden `RoomDataStreamOptions::with_legacy_client_implementation` builder and the hidden `SignalOptions::use_legacy_data_streams` field are gone; clients always advertise data streams v2 support.

## 0.1.7 (2026-10-23)

### Fixes

- fix: bump libwebrtc to webrtc-a545de4 to drop roots that trust the compromised eMudhra intermediates. The new build refreshes the TURN/TLS anchors compiled into `rtc_base/ssl_roots.h`, which no longer include `AAA Certificate Services`, the root that cross-signs the eight eMudhra intermediate CAs whose keys were disclosed on 2026-10-20.
- Fix the VAAPI H.264 encoder leaking VA parameter buffers on every frame; they are now destroyed after vaEndPicture().
- Stop the NVIDIA and Jetson encoders from reporting an already delivered frame as an encode error, which aborted the process in `SimulcastEncoderAdapter` when dynacast paused a simulcast layer or a track was unpublished mid-encode.

## 0.1.6 (2026-10-20)

### Fixes

- Outgoing data stream chunk and trailer packets now carry the stream's destination identities. Before, only the header was targeted, so the content of a stream sent to specific participants was delivered to everyone in the room.
- Synthesize index-based identifiers on platforms without hardware audio GUIDs and add index-based selection to PlatformAudio.
- Prefix every ffmpeg symbol bundled in the linux `libwebrtc.a` with `LK_`, so an application that also links its own FFmpeg, shared or static, no longer calls libwebrtc's copy or fails to link.
- Update `zip` to 9.0.4, as older releases have security issues and were temporarily yanked ([zip-rs/zip2#1075](https://github.com/zip-rs/zip2/issues/1075)).

#### Merge `livekit-uniffi` into `livekit-ffi`.

There is now one crate, one version and one release tag. The `livekit-uniffi`
crate, its changelog and its `livekit-uniffi/v*` tags are gone; its history is
preserved in git.

The crate carries two mutually exclusive surfaces, selected by feature:

- `room-apis` (default) is the existing protobuf / C-ABI surface, with
  `livekit` and libwebrtc. Everything that built `livekit-ffi` before builds it
  unchanged.
- `core-modules` is the former `livekit-uniffi` surface — access tokens, log
  forwarding, data tracks, data streams v2 — and depends on none of `livekit`,
  `libwebrtc`, `soxr-sys` or `imgproc`. What previously built `livekit-uniffi`
  now builds `-p livekit-ffi --no-default-features --features core-modules`.

`[package.metadata.platform-features]` in `livekit-ffi/Cargo.toml` records
which surface each downstream package build gets, and CI resolves its build
flags from it.

The merge itself requires no downstream SDK changes: the UniFFI namespace is
pinned per surface and every published package name is preserved. The one
difference is the native library the crate builds, `liblivekit_uniffi` →
`liblivekit_ffi`, which is loaded by name from the generated bindings and the
Dart build hook and needs no consumer change. The package renames are a
separate change, released alongside this one.

The UniFFI node package (`@livekit/uniffi`) is not built for now and will be
reintroduced separately; node bindings ship from `livekit-ffi-node-bindings`.

#### `livekit-protocol` exposes the conversation schema of a thread's database, as protocol's Go package

does: `agent::THREAD_SCHEMA_VERSION` and `agent::THREAD_SCHEMA_V1` (the SQL); the crates that
depend on it follow its bump.

#### Stop failing an RPC call whose reply arrives just after its recipient leaves the room.

Purging a call's pending ack and response the instant its recipient disconnects assumed that
"participant left" implies "no reply is coming". It does not: the disconnect is announced on the
signalling connection while the ack and response travel on the data channel, and nothing orders
the two. A recipient that leaves *because of* the RPC it just answered — `lk.sip.EndCall`, where
the SIP participant drops out of the room as soon as it has ended the call — produces that order
routinely, so the reply was discarded ("Response received for unexpected RPC request") and the
caller was handed `RecipientDisconnected` for a call that in fact succeeded.

A disconnect now shortens the wait instead of ending it: the pending entries stay registered for
up to 2 more seconds, so a reply already in flight still resolves the caller, and a recipient that
really did leave without replying still fails fast rather than running out the 15s response
timeout.

## 0.1.5 (2026-10-12)

### Features

- Add a capture source that ingests encoded video from a GStreamer pipeline.

### Fixes

- In-flight incoming data streams now error with `AbnormalEnd` when the room closes. Before, the stream readers ended cleanly, so `read_all()` returned truncated content as if it were complete.
- Keep pre-encoded video decodable when WebRTC drops frames: the passthrough encoder disables WebRTC's frame dropper, and after a frame is dropped before it, holds delta frames and requests a keyframe until the next keyframe arrives.
- Add `PeerConnection::set_bitrate` to set the send bitrate limits of a peer connection.
- Public API tokens minted from an API key and secret now grant every thread action (list, create, read, write, delete) across the project. Before, they carried only `all`, so creating or reading a thread was refused.
- Re-apply dynacast publishing layers after every publisher answer, so that layers paused by dynacast stay paused after a renegotiation.
- `Room::close` now leaves the room `Disconnected` and emits `RoomEvent::Disconnected` before it returns, and `Disconnected` is final: a reconnect still finishing in the background (a queued resume, or the republish after a full reconnect) can no longer bring a closed room back to `Connected` or emit `Reconnected` after `Disconnected`. Before, the room's event task could drop the engine's `Disconnected` event when the close signal won the race, so a closed room kept reporting `Connected` (or `Reconnecting`) about half the time.

#### `livekit-protocol` carries the `livekit.agent` package (`AgentSessionEvent`,

`SessionRequest`, the EOT inference messages), the agent threads messages
and the agent-db service; the crates that depend on it follow its bump.

## 0.1.4 (2026-10-07)

### Fixes

- Scale the `x-google-start-bitrate` hint by connection setup time: the 1 Mbps camera cap now applies to connections that set up within 1.5 s and ramps linearly down to 300 kbps at 3.5 s or slower; screen share stays uncapped.
- Increase the local and remote data track event buffers so a burst of track lifecycle events cannot fill the channel and deadlock room disconnect.
- Manage VAAPI displays across capability probes and encoder lifecycles so driver resources and threads are released without invalidating active encoders.
- Release local published tracks when a server-initiated room closure has already closed the peer connection.
- RoomClient::send_data now returns a Send future by dropping its RNG before awaiting.
- Never wait on the peer when closing a signal stream: a half-open socket (no FIN/RST) left the reader parked in `recv()` forever, so the resume never dialled and `Room::close` hung. `SignalStream::close` now aborts the parked reader and bounds the writer's Close flush.

## 0.1.3 (2026-09-24)

### Fixes

- Avoid panics when malformed RTC error headers contain non-ASCII text.
- Java version in libwebrtc was bumped by Google, downgrade it again for Unity 2022 build compatibility - #1456 (@MaxHeimbrock)
- refactor(signaling): explicit signal lifecycle state machine - #1402 (@lukasIO)
- Upgrade to prost 0.14 across the whole project - #1447 (@1egoman)

## 0.1.2 (2026-09-22)

### Features

- Add H.264 and H.265 access unit parsing.
- Add a capture source that renders a wall clock on the GPU.
- Add a capture source that renders a test pattern on the GPU.

### Fixes

- Correct the AppKit framework name so macOS linking works with case-sensitive SDK filesystems.
- Automatic capture thread join on drop
- Cleanup unused dependencies
- Fix reliable data channel replay: keep the full retry buffer across resumes, drop duplicate reliable packets, and ignore replayed chunks on uncompressed streams instead of failing with `MissedChunk`.
- Stop answering a server-initiated Leave (room deleted, duplicate identity) with a client Leave. The server has already ended the session and is closing the signalling socket, so the reply only ever produced the warning "dropping pass-through signal — no stream available" on every such disconnect.

#### Fix the RTC engine leaking when a room is dropped without being closed.

The engine's event task held a strong reference to the engine, while the signal that stops
that task lives inside the engine itself, so the two kept each other alive. An engine
dropped without an explicit `close()` released nothing: the session, both peer connections,
the WebRTC runtime, and the signal client with its open websocket all stayed resident for
the lifetime of the process, and the server kept its half of the session because the socket
was never closed. The task now holds a weak reference and stops on its own once the engine
is gone.

#### `EncryptionError::Failed` and `DecryptionError::Failed` carry a `reason` string and are no longer `flat_error`,

so a foreign `EncryptionProvider` or `DecryptionProvider` returning an error no longer aborts the process with
"Can't lift flat errors" -- a failed data track decrypt (no E2EE manager, key mismatch, corrupt frame) now 
drops the frame and leaves the room connected.

#### Report the reconnect reason to the server when resuming.

Resumes previously sent no reason, so server-side telemetry could not attribute why Rust
clients reconnect — every resume looked like `RR_UNKNOWN`. The engine now records what caused
the episode (signal disconnected, publisher failed, subscriber failed) and reports it on each
resume attempt. The v0 signalling path was also missing the `reconnect_reason` query parameter
entirely, so it would not have been reported even if a reason had been supplied.

#### Fix resume reporting success for a PeerConnection that had not recovered.

A resume decided recovery from `PeerConnectionState`, which keeps reading `Connected` for tens
of seconds after the far end goes away. A resume could therefore emit `Resumed` — and so
`RoomEvent::Reconnected` with `ConnectionState::Connected` — for a session whose subscriber
transport was dead, leaving applications with no signal that they had stopped receiving media.
A resume now requires each transport to have entered `Connected` since the resume began, or to
have held it throughout, rather than trusting the state it currently reports.

#### Write the `x-google-start-bitrate` hint once per publisher connection, and exempt screen share from the 1 Mbps cap.

libwebrtc reads this fmtp parameter per m-section but applies it to the shared `Call` (`WebRtcVideoSendChannel::ApplyChangedParams` -> `SetSdpBitrateParameters`), where `RtpBitrateConfigurator` holds one config for the whole peer connection. It retains `start_bitrate_bps` and re-applies it on network route changes (`RtpTransportControllerSend::OnNetworkRouteChanged`), so a WiFi-to-cellular handover re-seeds the estimator from the original hint with no renegotiation. Rewriting the value on later offers was therefore at best a no-op and at worst a restart of a converged bandwidth estimator; it is now written only on the first offer that carries local video, and only once that offer is accepted locally. A full reconnect builds a new peer connection and seeds the new estimator again. The initial offer sent with the JoinRequest in single PC mode never carries the hint: it is created before any track is published, so no target bitrate exists yet.

Screen share is no longer capped at 1 Mbps, matching client-sdk-js and client-sdk-android: unlike camera content, a screen share is published at a high bitrate so text stays legible, and a conservative start costs more than a brief overshoot.

## 0.1.1 (2026-09-10)

### Features

- Add a `livekit-capture` crate with the core abstractions for capturing video and publishing it to a LiveKit track.
