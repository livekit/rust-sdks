#!/bin/bash
set -e

cargo build --release

cargo run --bin uniffi-bindgen generate \
    --library ../target/release/liblivekit_uniffi.dylib \
    --language swift \
    --out-dir "generated/swift"

mv ./generated/swift/livekit_uniffiFFI.modulemap ./generated/swift/module.modulemap

rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-ios
rustup target add aarch64-apple-ios-sim
rustup target add x86_64-apple-ios

cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-ios
cargo build --release --target aarch64-apple-ios-sim
cargo build --release --target x86_64-apple-ios

# Combine iOS Simulator here

xcodebuild -create-xcframework \
    -library ../target/aarch64-apple-ios-sim/release/liblivekit_uniffi.a -headers ./generated/swift \
    -library ../target/aarch64-apple-ios/release/liblivekit_uniffi.a -headers ./generated/swift \
    -library ../target/aarch64-apple-darwin/release/liblivekit_uniffi.a -headers ./generated/swift \
    -output "../target/LiveKitFFI.xcframework"
