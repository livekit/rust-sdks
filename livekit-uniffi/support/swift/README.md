# Swift packaging

Details for the `cargo make swift-package` task. Both modes write to `./packages/swift/LiveKitUniFFI/`.

## Modes

**Local development** — default profile, produces a `Package.swift` that points at the unzipped xcframework via a relative `path:` so it can be consumed directly from `./packages/swift/LiveKitUniFFI/`:

```
cargo make swift-package
```

To consume it from a Swift project (e.g. `client-sdk-swift`), add this dependency entry to its `Package.swift` (and `Package@swift-6.2.swift`):

```swift
.package(name: "livekit-uniffi-xcframework", path: "../rust-sdks/livekit-uniffi/packages/swift/LiveKitUniFFI"),
```

Adjust the relative path to match your checkout layout. Don't commit this change — it's purely for local iteration.

**Release** — produces a zipped xcframework, computes its SHA256, and renders `Package.swift` / `Package@swift-6.2.swift` / podspec with a remote `url:` + `checksum:` pointing at the hosting repo release. Used by CI (`.github/workflows/uniffi-swift.yml`) to publish to [_livekit-uniffi-xcframework_](https://github.com/livekit/livekit-uniffi-xcframework):

```
SPM_VERSION=0.1.0 cargo make --profile release swift-package
```

`SPM_HOSTING_REPO` defaults to `livekit/livekit-uniffi-xcframework`; override it if forking.

## Local dependencies

In addition to `cargo-make`:

- Xcode + Command Line Tools (for `xcodebuild`, `lipo`, the iOS/macOS SDKs)
- Rust stable with these Apple targets: `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios`, `aarch64-apple-ios-macabi`, `x86_64-apple-ios-macabi`, `aarch64-apple-darwin`, `x86_64-apple-darwin`
- Rust nightly + `rust-src` component (cargo-swift falls back to `cargo +nightly -Zbuild-std` for tier-3 Apple targets — currently tvOS and visionOS):

  ```
  rustup toolchain install nightly --component rust-src
  ```

[_cargo-swift_](https://github.com/antoniusnaumann/cargo-swift) and [_tera-cli_](https://github.com/chevdor/tera-cli) are installed automatically by `cargo make` on first run.
