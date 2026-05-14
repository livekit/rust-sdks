# LiveKit UniFFI

Experimental FFI interface using [UniFFI](https://mozilla.github.io/uniffi-rs/latest/).

At this stage in development, this interface will not attempt to replace the existing FFI interface defined in [_livekit-ffi_](../livekit-ffi/). Instead, it will focus on exposing core business logic that can be cleanly modularized and adopted by client SDKs incrementally.

## Functionality exposed

- [x] Logging
- [x] Access token generation and verification

## Tasks

Binding generation and multi-platform builds are handled by [_cargo-make_](https://github.com/sagiegurari/cargo-make)—please install on your system before proceeding. For a full list of available tasks, see [_Makefile.toml_](./Makefile.toml) or run `cargo make --list-all-steps`. The most important tasks are summarized below:

### Swift

Generate Swift bindings and build a multi-platform XCFramework. The task supports two modes:

**Local development** — default profile, produces a `Package.swift` that points at the unzipped xcframework via a relative `path:` so it can be consumed directly from `./packages/swift/LiveKitUniFFI/`:

```
cargo make swift-package
```

**Release** — produces a zipped xcframework, computes its SHA256, and renders `Package.swift` / `Package@swift-6.2.swift` / podspec with a remote `url:` + `checksum:` pointing at the hosting repo release. Used by CI (`.github/workflows/uniffi-swift.yml`) to publish to [_livekit-uniffi-xcframework_](https://github.com/livekit/livekit-uniffi-xcframework):

```
SPM_VERSION=0.1.0 cargo make --profile release swift-package
```

`SPM_HOSTING_REPO` defaults to `livekit/livekit-uniffi-xcframework`; override it if forking. Both modes write to `./packages/swift/LiveKitUniFFI/`.

**Local dependencies** (in addition to `cargo-make`):

- Xcode + Command Line Tools (for `xcodebuild`, `lipo`, the iOS/macOS SDKs)
- Rust stable with these Apple targets: `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios`, `aarch64-apple-ios-macabi`, `x86_64-apple-ios-macabi`, `aarch64-apple-darwin`, `x86_64-apple-darwin`
- Rust nightly + `rust-src` component (cargo-swift falls back to `cargo +nightly -Zbuild-std` for tier-3 Apple targets — currently tvOS and visionOS):

  ```
  rustup toolchain install nightly --component rust-src
  ```

[_cargo-swift_](https://github.com/antoniusnaumann/cargo-swift) and [_tera-cli_](https://github.com/chevdor/tera-cli) are installed automatically by `cargo make` on first run.

### Node

Generate Node bindings:
```
cargo make node-package
```

To test them out, run `cd node_test && npx tsx index.ts`
