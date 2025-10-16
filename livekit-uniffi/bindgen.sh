#!/bin/bash
set -e

cargo build --release

cargo run --bin uniffi-bindgen generate --library ../target/release/liblivekit_uniffi.dylib --language swift --out-dir generated/swift
cargo run --bin uniffi-bindgen generate --library ../target/release/liblivekit_uniffi.dylib --language kotlin --out-dir generated/kotlin
cargo run --bin uniffi-bindgen generate --library ../target/release/liblivekit_uniffi.dylib --language python --out-dir generated/python
