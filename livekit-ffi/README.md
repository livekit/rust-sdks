# LiveKit FFI

Foreign function interface (FFI) bindings for LiveKit. The crate exposes two
**mutually exclusive** surfaces, selected by cargo feature for each sdk build
based on that given sdk's implementation.

## `room-apis` (default)

The protobuf / C-ABI room surface, plus a growing UniFFI one. Used by the SDKs
that have no WebRTC stack of their own, so this build embeds the full
[_livekit_](https://crates.io/crates/livekit) + libwebrtc stack:

- [Python](https://github.com/livekit/python-sdks)
- [NodeJS](https://github.com/livekit/node-sdks)
- [Unity](https://github.com/livekit/client-sdk-unity)

```sh
cargo build -p livekit-ffi --no-default-features --features 'room-apis rustls-tls-native-roots'
# or: cargo build -p livekit-ffi
```

Mode specific source lives in `src/room_apis/`. `room-apis` gates the shared
surface; select exactly one transport. Defaults and TLS features enable
`room-apis-bundled-transport`, which preserves the bundled network stack.

If you previously built with only `--no-default-features --features room-apis`,
use `--no-default-features --features room-apis-bundled-transport` instead.
Selecting the surface alone now fails with a migration diagnostic. Existing
TLS-based commands and the host command below are unchanged.

### `room-apis-host-transport`

The same room surface, but the host application supplies the network. The
artifact contains no bundled WebSocket, HTTP or TLS client (no `reqwest`,
`tokio-tungstenite` or `rustls`). WebRTC media stays in Rust. The build also
exports the UniFFI interface of [_livekit-net_](../livekit-net): `WsClient`,
`WsConnection`, `HttpClient`, `set_ws_client` and `set_http_client`.

```sh
cargo build -p livekit-ffi --no-default-features --features room-apis-host-transport
```

- Build it alone. `room-apis-bundled-transport`, TLS features, `tracing` and `capture` link
  the bundled stack again, so a `compile_error!` rejects them beside it.
- Register both clients one time, at startup, before the first connect. The
  registry is process-wide, the first registration stays, and a later one is
  ignored without an error. One owner in the application must do it.
- Without a client, connect fails with
  `NativeSignalFailure::TransportNotConfigured`. There is no fallback.
- No platform row uses this feature yet. `ffi_features.py` accepts only
  `room-apis` or `core-modules` in a row.

## `core-modules`

Core business logic exposed purely over
[UniFFI](https://mozilla.github.io/uniffi-rs/latest/): access tokens, log
forwarding, data tracks and data streams v2. Used by the SDKs that already ship
their own WebRTC stack. This build therefore depends on none of `livekit`,
`libwebrtc`, `soxr-sys` or `imgproc`, so an application does not ship two
WebRTC stacks:

- [Swift](https://github.com/livekit/client-sdk-swift)
- [Android](https://github.com/livekit/client-sdk-android)
- [Flutter](https://github.com/livekit/client-sdk-flutter)

```sh
cargo build -p livekit-ffi --no-default-features --features core-modules
```

Mode specific source lives in `src/core_modules/`.

## Which platform gets which

`[package.metadata.platform-features]` in [Cargo.toml](./Cargo.toml) is the
single source of truth, and CI resolves its build flags from it:

```sh
.github/scripts/ffi_features.py --list
```

## Things that bite

- **Never enable both features.** Each pins its own UniFFI namespace via
  `setup_scaffolding!`, and a crate may declare only one. A `compile_error!` in
  `src/lib.rs` enforces this; another rejects selecting neither.
- **The cargo-make tasks below take no `-p`** and rely on the working
  directory, so they need explicit feature flags or they build the default
  livekit-ffi/room-apis and all of libwebrtc. `Makefile.toml` resolves those from
  the table above — do not write them out by hand.
- **Gate shared room code on `room-apis`.** Both transport selections imply it.
- **`build.rs` is gated on `room-apis`.** It downloads libwebrtc, configures the
  linker, and panics on any target it does not recognise — visionOS, tvOS and
  Mac Catalyst among them. That gate is what makes the core-modules Apple
  builds possible.
- **Verify UniFFI API changes by compiling the Kotlin bindings**
  (`cargo make android-package`). A green `cargo build` proves nothing there;
  see the UniFFI section of the root [AGENTS.md](../AGENTS.md) for the specific
  naming traps.

## Tasks

Binding generation and multi-platform builds are handled by
[_cargo-make_](https://github.com/sagiegurari/cargo-make) — please install it
before proceeding. For the full list see [_Makefile.toml_](./Makefile.toml) or
run `cargo make --list-all-steps`. The most important are summarized below.

### Swift

Generate Swift bindings and build a multi-platform XCFramework:

```
cargo make swift-package
```

For a fast, debuggable variant (macOS-only, unstripped), run
`cargo make swift-package-debug` — see
[support/swift/DEBUGGING.md](./support/swift/DEBUGGING.md).

See [support/swift/README.md](./support/swift/README.md) for debugging vs.
release modes, consumer integration, and prerequisites (Xcode, Rust Apple
targets).

Build the complete room package (Rust-owned WebRTC) through the same pipeline:

```sh
cargo make swift-room-package       # bundled WebSocket/HTTP and webpki TLS roots
cargo make swift-room-host-package  # host registers WsClient and HttpClient
```

Room packages support macOS 11 and iOS 14 (ARM64 device and simulator). They do not
use the core-only size budget or include Catalyst, tvOS, or visionOS.
For a single local slice, use
`SPM_PLATFORMS=macos SPM_TARGET_FLAG="--target aarch64-apple-darwin" cargo make swift-room-host-package`.
The output is `packages/swift/LiveKitFFI`, including bindings, XCFramework,
privacy manifest, and package manifests. Each invocation replaces that output;
copy it elsewhere before building another transport. Release builds use the same
`--profile release`, `SPM_VERSION`, and `SPM_HOSTING_REPO` options as core builds.

### Android

Build native libraries, Kotlin bindings, and a release AAR:

```
cargo make android-package                              # debug .so in release AAR
cargo make --profile release android-package            # release .so (CI / publishing)
cargo make android-package-local                        # + publish to Maven Local
```

See [support/android/README.md](./support/android/README.md) for prerequisites
(Android SDK/NDK).

### Dart

```
cargo make dart-package
```

See [support/dart/README.md](./support/dart/README.md).

## Native room audio ownership

Manual PCM creation does not itself request microphone capture. However, core recording
is process-global: an explicitly created microphone track anywhere in the process can
enable recording and mix physical microphone input into later manual PCM tracks in any
session. The FFI does not fix or isolate that core behavior. Independent receive-only
hardware output remains blocked until public core separates output from capture.

A plain Cargo privacy-test pass establishes event/command progress only. The recorded
receive-first no-ADM evidence requires the LLDB guard with all three acquisition/recording
breakpoints resolved and both same-room/other-room PCM checkpoints reached. It does not
cover processes that explicitly activate a microphone.

## Native room controls

`FfiRoom.publish_dtmf(code, digit, destination_identities)` sends through
public core's reliable DTMF path; empty destinations use core broadcast semantics.
Core sends this packet without data E2EE even in encrypted rooms. Incoming packets
appear as `NativeRoomEvent::DtmfReceived { code, digit, participant_identity }`;
optional fields retain core absence. Event payloads count against cursor bounds.

`FfiRoom.set_track_subscription_permissions(all_participants_allowed,
permissions)` accepts `NativeTrackPermission { participant_identity, allow_all,
allowed_track_sids }`. Invalid SIDs fail with `InvalidState`; successful completion
means core stored the desired permissions; it does not guarantee the signaling request
was sent or acknowledged by the SFU. Current core
may retain stale permission/attached-track status after revoking an existing subscription;
the wrapper does not synthesize status or callbacks to conceal that limit.

`NativePublicationHandle.set_video_quality(NativeVideoQuality::{Low, Medium, High})`
requires a remote video publication and requests the highest acceptable simulcast
layer. Core ignores requests for non-simulcast tracks and may select lower quality
under bandwidth pressure. This does not add FPS, volume or server-layer diagnostics.
These commands directly await core operations through UniFFI. The FFI runtime helper
runs core work off the foreign executor, joining a caller-owned runtime hop. There is
no command supervisor, admission queue or result-forwarding channel.
No transcription export or private-core adapter is included.

Reader `wait_closed` remains a room-apis-only export; core-modules retains the shipped
reader protocols without new requirements. It observes closure independently and does
not establish ordering against room event-cursor opens.

### Thin typed room boundary

`FfiRoom` is independently usable through generated UniFFI bindings. It owns the
core connection and exposes commands without a native SDK wrapper. The separate
protobuf `server::room::FfiRoom` is unchanged.

The one connect/event task is retained because dropping a core connect future has
no documented transport-cleanup guarantee. Cancellation returns promptly and the driver
releases room-owned media before awaiting a late connect result. `disconnect` waits up
to ten seconds for that driver and returns `ShutdownTimeout` if cleanup is still running.
The room retains the task handle and the complete core close future: a later disconnect
can await completion again. A permanently stalled core can retain this owner until process
exit; a timeout does not mark teardown complete or finish the event cursor. Successful
late connections are closed without installing their snapshot or emitting connected events.
An unwinding driver panic reports an internal shutdown error; release builds abort on panic.
Ordinary commands are
owned by their UniFFI futures and use core completion/error behavior on disconnect.
Dropping a caller aborts its runtime hop via tokio-util; ordinary completion joins it.
Core track-publication receivers own their pending registry entries, so dropping either
an ordinary or startup publication removes only that registration. The low-level Rust
`RtcEngine::connect` startup receivers are now awaitable `PendingTrack` guards rather
than raw Tokio oneshot receivers. This does not retract
an AddTrack request already sent to the server or guarantee server-side rollback.
The hop is necessary because synchronous WebRTC work within a poll must not block
Swift’s cooperative executor.

Snapshots are foreign value conversions, not a second mutable room model. Core
participant/publication handles retain membership in receiver order so the initial
snapshot cannot include a later join before its event. Values are read from core on
demand and can be newer than the event. Revisions delimit cursor start/overflow
recovery snapshots and ordered updates. The bounded queue carries payload events
that cannot be reconstructed from core state; its limits protect slow consumers.
Media handles retain source/sink ownership with weak room back-references. Error
adapters preserve core codes and convert non-exportable payloads (HTTP status,
borrowed strings and WebRTC errors); v2 error classification belongs in Swift.
