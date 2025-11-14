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
