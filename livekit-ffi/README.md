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

Mode specific source lives in `src/room_apis/`.

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
- **Gate room code on `__room-surface`, not `room-apis`.** `__room-surface` is
  the room surface with no network stack selected; `room-apis` adds the
  bundled one.
- **`build.rs` is gated on `__room-surface`.** It downloads libwebrtc, configures the
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

Combine an existing room-apis Swift package and matching, already-built iOS frameworks
without rebuilding native code (all three must have verified matching metadata):

```sh
sh livekit-ffi/support/swift/package-room-apple.sh /path/to/mac-package \
  /path/to/device/RustLiveKitFFI.framework /path/to/sim/RustLiveKitFFI.framework \
  /path/to/new-output
```

This preserves macOS framework symlinks, guards the known manifest minimums before
setting iOS14/macOS11, and strictly verifies all three signatures before and after zip
extraction. The output directory must not exist; it receives the package and `LiveKitFFI.zip`.


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

`NativeRoomSession.publish_dtmf(code, digit, destination_identities)` sends through
public core's reliable DTMF path; empty destinations use core broadcast semantics.
Core sends this packet without data E2EE even in encrypted rooms. Incoming packets
appear as `NativeRoomEvent::DtmfReceived { code, digit, participant_identity }`;
optional fields retain core absence. Event payloads count against cursor bounds.

`NativeRoomSession.set_track_subscription_permissions(all_participants_allowed,
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
All three commands use the session's existing task admission and teardown ownership.
No transcription export or private-core adapter is included.

Reader `wait_closed` remains a room-apis-only export; core-modules retains the shipped
reader protocols without new requirements. It observes closure independently and does
not establish ordering against room event-cursor opens.
