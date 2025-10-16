# LiveKit UniFFI

Experimental FFI interface using [UniFFI](https://mozilla.github.io/uniffi-rs/latest/).

At this stage in development, this interface will not attempt to replace the existing FFI interface defined in [_livekit-ffi_](../livekit-ffi/). Instead, it will focus on exposing core business logic that can be cleanly modularized and adopted by client SDKs incrementally.

## Functionality exposed

- [x] Access token generation and verification

## Generating bindings

Use the _bindgen.sh_ script to generate language bindings for Swift, Kotlin, and Python.

Later, this script will integrate community binding generators to support more languages.
