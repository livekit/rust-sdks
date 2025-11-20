# LiveKit UniFFI

Experimental FFI interface using [UniFFI](https://mozilla.github.io/uniffi-rs/latest/).

At this stage in development, this interface will not attempt to replace the existing FFI interface defined in [_livekit-ffi_](../livekit-ffi/). Instead, it will focus on exposing core business logic that can be cleanly modularized and adopted by client SDKs incrementally.

## Functionality exposed

- [x] Access token generation and verification

## Generating bindings

Use the _bindgen.sh_ script to generate language bindings for Swift, Kotlin, and Python.

Later, this script will integrate community binding generators to support more languages.

## Python test

See the _python_test_ for a simple example of consuming the generated bindings. You will need to manually copy the compiled _livlivekit_uniffi_ to the same directory as the generated Python bindings before running—this will be automated shortly.
