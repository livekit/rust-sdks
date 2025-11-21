# LiveKit UniFFI

Experimental FFI interface using [UniFFI](https://mozilla.github.io/uniffi-rs/latest/).

At this stage in development, this interface will not attempt to replace the existing FFI interface defined in [_livekit-ffi_](../livekit-ffi/). Instead, it will focus on exposing core business logic that can be cleanly modularized and adopted by client SDKs incrementally.

## Functionality exposed

- [x] Access token generation and verification

## Tasks

Binding generation and multi-platform builds are handled by [_cargo-make_](https://github.com/sagiegurari/cargo-make)—please install on your system before proceeding. For a full list of available tasks, see [_Makefile.toml_](./Makefile.toml) or run `cargo make --list-all-steps`. The most important tasks are summarized below:

### Swift

Generate Swift bindings and build a multi-platform XCFramework:
```
cargo make xcframework
```

TODO: other languages