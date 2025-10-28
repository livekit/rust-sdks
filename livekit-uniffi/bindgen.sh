#!/bin/bash
set -e

cargo build --release

bindgen() {
  local lang=$1
  # TODO: set the library extension based on platform (i.e., .so, .dylib, .dll)
  cargo run --bin uniffi-bindgen generate \
    --library ../target/release/liblivekit_uniffi.dylib \
    --language "$lang" \
    --out-dir "generated/$lang"
}

bindgen swift
bindgen kotlin
bindgen python


cargo install uniffi-bindgen-cs --git https://github.com/NordSecurity/uniffi-bindgen-cs --tag v0.10.0+v0.29.4
uniffi-bindgen-cs --library ../target/release/liblivekit_uniffi.dylib --out-dir generated/csharp