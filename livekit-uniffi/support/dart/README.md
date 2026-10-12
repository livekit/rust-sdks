# livekit_uniffi

Dart bindings for the LiveKit Rust SDK core, generated from the
[`livekit-uniffi`](https://github.com/livekit/rust-sdks/tree/main/livekit-uniffi)
crate with [UniFFI](https://mozilla.github.io/uniffi-rs/) and
[uniffi-dart](https://github.com/Uniffi-Dart/uniffi-dart).

This is a low-level package. It is consumed by
[`livekit_client`](https://pub.dev/packages/livekit_client) and is not
intended to be used directly in applications.

## How it works

The bindings call into a prebuilt Rust dynamic library through Dart's
[Native Assets](https://dart.dev/tools/hooks). The package's `hook/build.dart`
resolves the library for your target platform at build time, downloading it
from the matching [`livekit-uniffi` release](https://github.com/livekit/rust-sdks/releases)
and verifying its SHA-256 checksum. No Rust toolchain is required to consume
this package.

Requires Dart >= 3.10 (Flutter >= 3.38), where build hooks and code assets are
stable. There is no web support: guard usage behind a conditional import.

## Supported platforms

macOS (arm64, x64), iOS (device and simulator), Android (arm64, armv7, x64),
Linux (arm64, x64), and Windows (arm64, x64).

## Development

This package is generated; do not edit it directly. To build it from source:

```sh
cd rust-sdks/livekit-uniffi
cargo make dart-package    # generates packages/dart with a locally built library
```

See the [crate README](https://github.com/livekit/rust-sdks/tree/main/livekit-uniffi)
for details. Issues and pull requests go to
[livekit/rust-sdks](https://github.com/livekit/rust-sdks).
