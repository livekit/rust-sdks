#!/bin/bash
set -e

cargo build --release

cargo run --bin uniffi-bindgen generate \
    --library ../target/release/liblivekit_uniffi.dylib \
    --language swift \
    --out-dir "generated/swift"

# Required for xcframework
mv ./generated/swift/livekit_uniffiFFI.modulemap ./generated/swift/module.modulemap

RUSTUP_TARGETS=(
    aarch64-apple-darwin
    x86_64-apple-darwin
    aarch64-apple-ios
    aarch64-apple-ios-sim
    x86_64-apple-ios
    aarch64-apple-ios-macabi
    x86_64-apple-ios-macabi
)

TIER3_TARGETS=(
    aarch64-apple-tvos
    aarch64-apple-visionos
    aarch64-apple-tvos-sim
    aarch64-apple-visionos-sim
)

for target in "${RUSTUP_TARGETS[@]}"; do
    rustup target add "$target"
done

for target in "${RUSTUP_TARGETS[@]}"; do
    cargo build --release --target "$target"
done

for target in "${TIER3_TARGETS[@]}"; do
    cargo +nightly build -Zbuild-std=std,panic_abort --release --target="$target"
done

UNIVERSAL_BINARIES=(
    "ios-simulator:aarch64-apple-ios-sim:x86_64-apple-ios"
    "macos:aarch64-apple-darwin:x86_64-apple-darwin"
    "ios-macabi:aarch64-apple-ios-macabi:x86_64-apple-ios-macabi"
)

for config in "${UNIVERSAL_BINARIES[@]}"; do
    IFS=':' read -r output_dir arch1 arch2 <<< "$config"
    mkdir -p "../target/${output_dir}/release"
    lipo -create \
        "../target/${arch1}/release/liblivekit_uniffi.a" \
        "../target/${arch2}/release/liblivekit_uniffi.a" \
        -output "../target/${output_dir}/release/liblivekit_uniffi.a"
done

rm -rf ../target/LiveKitFFI.xcframework

XCFRAMEWORK_LIBS=(
    "../target/aarch64-apple-ios/release/liblivekit_uniffi.a"
    "../target/ios-simulator/release/liblivekit_uniffi.a"
    "../target/macos/release/liblivekit_uniffi.a"
    "../target/ios-macabi/release/liblivekit_uniffi.a"
    "../target/aarch64-apple-tvos/release/liblivekit_uniffi.a"
    "../target/aarch64-apple-visionos/release/liblivekit_uniffi.a"
    "../target/aarch64-apple-tvos-sim/release/liblivekit_uniffi.a"
    "../target/aarch64-apple-visionos-sim/release/liblivekit_uniffi.a"
)

XCFRAMEWORK_ARGS=()
for lib in "${XCFRAMEWORK_LIBS[@]}"; do
    XCFRAMEWORK_ARGS+=(-library "$lib" -headers ./generated/swift)
done

xcodebuild -create-xcframework \
    "${XCFRAMEWORK_ARGS[@]}" \
    -output "../target/LiveKitFFI.xcframework"
