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
their own WebRTC stack, so this build depends on none of `livekit`,
`libwebrtc`, `soxr-sys` or `imgproc` — which is what lets it target visionOS,
tvOS and Mac Catalyst:

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
.github/scripts/platform_features.py --list
```

## Things that bite

- **Never enable both features.** Each pins its own UniFFI namespace via
  `setup_scaffolding!`, and a crate may declare only one. A `compile_error!` in
  `src/lib.rs` enforces this; another rejects selecting neither.
- **The cargo-make tasks below take no `-p`** and rely on the working
  directory, so they need explicit feature flags or they build the default
  livekit-ffi/room-apis and all of libwebrtc. `Makefile.toml` resolves those from
  the table above — do not write them out by hand.
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
