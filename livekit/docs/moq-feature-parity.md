# Native MoQ feature parity with WebRTC

**No. Selecting MoQ preserves the Rust room API, but it changes which events can fire and what some events tell you.** The core video and DataTrack lifecycle works across all four publisher/subscriber combinations. A visible MoQ viewer now triggers the publisher’s first-subscriber event. A MoQ publisher can wait for that notification before capturing its first frame. Legacy messaging and room E2EE remain the largest differences for applications considering the transport switch.

This report compares `RoomTransport::Moq` with the default `RoomTransport::WebRtc` on native desktop/server. It covers all 44 public Rust `RoomEvent` variants and the transport features that affect a room application. It describes the November 24, 2026 working trees based on Rust SDK `eb74c3d7` and SFU `c5cfc84b`, with shared protocol `a609e7a53312`. Both transports use LiveKit WebSocket signaling. A third-party client consuming a MoQ media catalog needs LiveKit signaling and its own event handling to observe room membership and publication metadata. [Room API and event definitions](../src/room/mod.rs) · [MoQ integration guide](moq.md)

## What the four transport combinations delivered

Each row used a fresh local room, one visible publisher, one subscriber and both Rust SDK event receivers. The probe published VP8 video and a named DataTrack. It exercised mute and unmute, participant metadata, name and attribute changes, a reliable legacy data send, and withdrawal of both tracks. “Yes” and “No” below are observations. The probe waited five seconds after the send while the video subscription remained active.

| Publisher → subscriber | Viewer received `TrackSubscribed` | Publisher received `LocalTrackSubscribed` | Viewer received DataTrack publish and unpublish | Publisher's legacy send succeeded | Viewer received `DataReceived` |
| --- | --- | --- | --- | --- | --- |
| WebRTC → WebRTC | Yes | Yes | Yes | Yes | Yes |
| WebRTC → MoQ | Yes | Yes | Yes | Yes | No |
| MoQ → WebRTC | Yes | Yes | Yes | No | No |
| MoQ → MoQ | Yes | Yes | Yes | No | No |

A separate probe sent a continuous 440 Hz tone through a 48 kHz programmatic microphone. Both publisher and viewer identified the publisher as active in all four combinations. Viewers on both transports also received the remote publisher’s quality. The Rust MoQ encoder measures the PCM in each Opus packet and writes the audio level and encoder voice flag into the LOC object. [LOC audio level](https://www.ietf.org/archive/id/draft-ietf-moq-loc-05.html#section-2.3.3.2)

The SFU registers a MoQ receiver tap after accepting the media subscription. It uses the WebRTC publisher-membership callbacks for speaker and quality updates and the existing once-per-publication callback for visible subscribers. A hidden subscriber can receive media without triggering that publisher notification. Subscription cleanup is scoped to the attachment generation.

For application data, handle `DataTrackPublished` and call `RemoteDataTrack::subscribe()` to receive frames. Both transports expose frames on the returned stream. Neither emits a room event per DataTrack frame. A successful legacy send from a WebRTC publisher acknowledges the local send operation and gives no assurance of delivery to a MoQ recipient. [Remote DataTrack API](../../livekit-datatrack/src/remote/mod.rs) · [Mixed-room frame exchange](../tests/moq_test.rs)

## Room event inventory

**31 supported, 2 limited, 11 unavailable.** Supported means the ordinary trigger exists for the supported media or data profile. Limited means the notification depends on transport direction or carries less information. Unavailable means the MoQ participant or media path lacks the trigger. One of those events, `Moved`, also lacks a working service in this SFU checkout. “Probe” refers to the local comparisons in this report, “suite” to the repository tests, and “source” to a traced implementation path.

| Rust room event | MoQ status | Trigger, evidence and limits |
| --- | --- | --- |
| `Connected`, `ConnectionStateChanged`, `Disconnected` | Supported | Probe. The SDK reported join and close on both transports. The initial participant/publication snapshot is part of `Connected`. A failed initial MoQ attachment returns a join error. |
| `Reconnecting`, `Reconnected` | Supported | Probe and suite. The room engine reports recovery of signaling or a full reconnect. A successful QUIC path change alone keeps the room connected. |
| `ParticipantConnected`, `ParticipantActive`, `ParticipantDisconnected`, `ParticipantsUpdated` | Supported | Probe. A remote join, transition to active, departure and participant updates use the shared room state. Active membership does not imply support for legacy data delivery. |
| `RoomUpdated`, `RoomMetadataChanged` | Supported | Room updates observed in the probe. Source confirms a metadata change in the signaled room info also emits the specific metadata event. |
| `ParticipantMetadataChanged`, `ParticipantNameChanged`, `ParticipantAttributesChanged` | Supported | Probe. The other participant saw each change in all four combinations. |
| `ParticipantPermissionChanged`, `TokenRefreshed` | Supported | Source. Both use shared signaling handlers. The suite separately exercises media and DataTrack behavior after grant changes. |
| `LocalTrackPublished`, `LocalTrackUnpublished` | Supported | Probe. A successful local media publication and its explicit removal use the same app callbacks. |
| `LocalTrackRepublished` | Supported | Probe. A forced full reconnect on each transport produced the previous and replacement video SIDs in the event. The SDK reuses the media source. A surviving QUIC path change keeps the original publication. |
| `TrackPublished`, `TrackUnpublished` | Supported | Probe. Remote publication metadata arrives over signaling. A publication can be visible even when its rendition cannot be consumed over MoQ. |
| `TrackSubscribed`, `TrackUnsubscribed` | Supported | Probe and suite. The SDK matches the MoQ decoder track to its signaled publication. Withdrawal or permission revocation can detach it. Subscription indicates track attachment. The first decoded frame arrives separately. |
| `TrackMuted`, `TrackUnmuted` | Supported | Probe. The publisher and subscriber observed the video mute state change in both directions. |
| `DataTrackPublished`, `DataTrackUnpublished` | Supported | Probe. These are remote logical DataTrack lifecycle events. Local publication returns a handle. The suite independently verifies fragmented payload delivery. |
| `DataChannelBufferedAmountLowThresholdChanged` | Supported | Probe. Changing the SDK threshold emits the event on both transports. Its payload describes the configured SCTP threshold. It conveys no QUIC queue or drain information. |
| `LocalTrackSubscribed` | Supported | Probe and suite. The first visible viewer triggers this once for either publisher transport. The suite verifies a MoQ publisher can wait for the notification before sending a video frame. |
| `ActiveSpeakersChanged` | Supported | Microphone probe and suite. The server applies its usual level threshold and subscription scope. The native SDK supplies the level on Opus objects. A third-party MoQ publisher must supply the optional LOC audio level to participate in speaker detection. |
| `ConnectionQualityChanged` | Supported | Probe, suite and SFU scoring test. The viewer receives its own and subscribed publishers’ updates. The SFU includes the live MoQ connection’s smoothed RTT and its recent outbound QUIC packet-loss fraction in the participant score. This is a path estimate. Per-track decoded quality and client-side uplink loss are separate measurements. |
| `ParticipantEncryptionStatusChanged` | Limited | Source. A mixed-room client's participant model can report the encryption state advertised by a remote publication. MoQ filters encrypted renditions and the SDK rejects room E2EE in MoQ mode. |
| `TrackSubscriptionFailed` | Limited | A refused advertised rendition reports its publisher, track SID and reason. Request response timeouts, unsupported catalog formats and decoder failures also report a track error. A decoder failure detaches an already attached track. The SDK can retry on a new catalog and suppresses duplicate failures while the same rendition remains unavailable. A publication omitted from the authorized catalog has no per-track MoQ result. A session-wide failure goes through room reconnect. |
| `DataReceived`, `ChatMessage`, `TranscriptionReceived`, `SipDTMFReceived` | Unavailable | The probe verified the user-packet case. Source shows the built-in chat, transcription and SIP DTMF handlers also require legacy data packets. |
| `ByteStreamOpened`, `TextStreamOpened` | Unavailable | Source. The data-stream readers depend on legacy packet headers, chunks and trailers. Sending bytes or text through a DataTrack gives the application a DataTrack frame. |
| `StreamHeaderReceived`, `StreamChunkReceived`, `StreamTrailerReceived` | Unavailable | Source. The deprecated low-level events use the same missing packet path. |
| `E2eeStateChanged` | Unavailable | Source. MoQ has transport encryption with TLS. The Rust room rejects the E2EE configuration required to install a media frame cryptor. |
| `Moved` | Unavailable | Source. The SFU scopes the MoQ attachment to one room and rejects participant moves. The Rust SDK handles a WebRTC room-move signal on backends that implement it. This SFU's own `RoomManager.MoveParticipant` returns an unimplemented error for both transports. |

The SDK sources for the inventory are the [room dispatcher](../src/room/mod.rs), [session signaling and data-packet handlers](../src/rtc_engine/rtc_session.rs), [remote participant attachment](../src/room/participant/remote_participant.rs), [participant encryption status](../src/room/participant/mod.rs) and [MoQ catalog reconciliation](../src/rtc_engine/moq.rs). The corresponding SFU routing and reporting paths are in `pkg/rtc/participant_moq.go`, `pkg/rtc/participant_signal.go`, `pkg/rtc/participant.go`, `pkg/rtc/room.go` and `pkg/service/roommanager.go`.

## Feature parity beyond events

| Application feature | Default WebRTC | Native MoQ |
| --- | --- | --- |
| Video formats and mixed rooms | Negotiated RTP media | H.264, H.265, VP8, VP9 and AV1 verified in both mixed directions and between two MoQ participants. Both endpoints need the relevant codec. |
| Video quality selection | Simulcast, layer selection and adaptation where codec, sender and receiver support them | The SDK publishes one rendition. The SFU forwards the base spatial layer from a layered WebRTC publication. A quality or dimension request cannot select a higher MoQ rendition. RTP frame trailers are unsupported. |
| Audio | Negotiated WebRTC audio and RTP options | Opus with a native programmatic source is verified. RED is disabled on this publishing path. The sender supplies audio levels for active-speaker detection. |
| Application data | DataTracks plus legacy data, chat, RPC and text/byte streams | DataTracks only. MoQ participants always have DataTracks enabled, subject to the publisher and subscriber grants. Built-in RPC and streams depend on the legacy packet transport. |
| DataTrack delivery under loss | The SDK creates an unordered SCTP channel with zero retransmissions. | Each track uses QUIC DATAGRAM with its own MoQ alias. The publisher can set its send priority before or after publication. Missing chunks can cost a frame. Neither QUIC nor the SDK retransmits DataTrack payloads. A successful push reports local acceptance. |
| Access control | LiveKit room grants and publication/subscription permissions | Same signaling authority. The SFU also checks MoQ media and data access. The suite exercises revoke/regrant on an existing connection. |
| Encryption | WebRTC transport encryption and optional SDK room E2EE | QUIC with TLS 1.3. The room E2EE option and encrypted media subscriptions are unavailable. |
| Statistics | Peer connection and RTP sender/receiver statistics | `Room::get_stats()` reads the idle publisher peer connection and has no subscriber peer connection. Per-track stats require an RTP transceiver. The room quality event incorporates SFU-side QUIC measurements. The Rust stats APIs do not expose the raw counters. |
| Network handoff and full reconnect | The room engine can resume signaling or perform a full reconnect | A live QUIC connection can migrate to a new local address at the same SFU, preserving tracks and subscriptions. Signaling may reconnect independently. A lost attachment requires a fresh connection and republish. |
| Browser and third-party clients | Browser WebRTC participants can use the same room | Native QUIC is opt-in for the Rust client and SFU. An independent MoQ client can consume cataloged media after LiveKit signaling. Browser MoQ and a standalone MoQ room-event feed are outside this endpoint's profile. |

## Verification scope

The ad hoc event probes ran against one local SFU with MoQ and DataTracks enabled. The probe observed event presence on each side. It did not assert a global event order or simulate every administrative action. Signaling and the MoQ catalog arrive independently. The SDK has a regression test for media arriving before the corresponding publication metadata. [Room lifecycle test](../src/room/lifecycle_tests.rs)

The repository's six MoQ integration scenarios passed in the reviewed working tree. With a separate SFU capped at one video subscription, the second video produced a single failure and attached after the first publication was removed. The original room stayed connected. They cover decoded video in all five codecs, Opus, fragmented 128 KiB DataTrack frames, changed grants, full reconnect, a simulated QUIC handoff and an independent media subscriber. The handoff test also passed with a two-second simulated outage. This evidence covers local interoperability and simulated recovery. Physical Wi-Fi/cellular handoffs and release-scale load were outside the tests. The integration guide includes the command and required SFU settings. [MoQ integration tests](../tests/moq_test.rs) · [Run the suite](moq.md#verify-an-integration)
