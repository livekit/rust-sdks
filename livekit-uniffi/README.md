# LiveKit UniFFI

Experimental FFI interface using [UniFFI](https://mozilla.github.io/uniffi-rs/latest/).

At this stage in development, this interface will not attempt to replace the existing FFI interface defined in [_livekit-ffi_](../livekit-ffi/). Instead, it will focus on exposing core business logic that can be cleanly modularized and adopted by client SDKs incrementally.

## Functionality exposed

- [x] Logging
- [x] Access token generation and verification

## Tasks

Binding generation and multi-platform builds are handled by [_cargo-make_](https://github.com/sagiegurari/cargo-make)—please install on your system before proceeding. For a full list of available tasks, see [_Makefile.toml_](./Makefile.toml) or run `cargo make --list-all-steps`. The most important tasks are summarized below:

### Swift

Generate Swift bindings and build a multi-platform XCFramework:
```
cargo make swift-package
```

For a fast, debuggable variant (macOS-only, unstripped), run `cargo make swift-package-debug` — see [support/swift/DEBUGGING.md](./support/swift/DEBUGGING.md).

See [support/swift/README.md](./support/swift/README.md) for debugging vs. release modes, consumer integration, and prerequisites (Xcode, Rust Apple targets).

### Node

Generate the node packages:
```
cargo make node-package
cargo make node-native-package
cargo make node-setup-workspace
```

Each is a separate invocation: `cargo make` takes one task name, and any further words are
passed to it as arguments rather than run as tasks.

This produces `target/packages/node` (`@livekit/uniffi`) and
`target/packages/node-native/<triple>` (`@livekit/uniffi-<node-triple>`), sharing
a generated pnpm workspace. Both paths are relative to the **workspace root**,
not to this crate — `target/` belongs to the workspace, and there is no
`livekit-uniffi/target/`. The main package resolves the native library through
its per-arch sibling, so both are needed.

To exercise them:
```
cargo make node-package-test
```

To check the published shape — `files`, `exports`, and platform-package
resolution, none of which a workspace link goes through:
```
cargo make node-pack-test
```

CI (`.github/workflows/uniffi-node-test.yml`) runs both.

### Android

Build native libraries, Kotlin bindings, and a release AAR:

```
cargo make android-package                              # debug .so in release AAR
cargo make --profile release android-package            # release .so (CI / publishing)
cargo make android-package-local                        # + publish to Maven Local
```

See [support/android/README.md](./support/android/README.md) for prerequisites (Android SDK/NDK).
