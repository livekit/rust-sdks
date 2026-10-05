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
