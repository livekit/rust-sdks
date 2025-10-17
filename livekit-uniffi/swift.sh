#!/bin/bash
set -e

UNIFFI_MODULE="livekit_uniffi"
LIB_NAME="lib${UNIFFI_MODULE}"
XCFRAMEWORK_NAME="LiveKitFFI"

cargo build --release

cargo run --bin uniffi-bindgen generate \
    --library "../target/release/${LIB_NAME}.dylib" \
    --language swift \
    --out-dir "generated/swift"

# Required for xcframework
mv ./generated/swift/${UNIFFI_MODULE}FFI.modulemap ./generated/swift/module.modulemap

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
        "../target/${arch1}/release/${LIB_NAME}.a" \
        "../target/${arch2}/release/${LIB_NAME}.a" \
        -output "../target/${output_dir}/release/${LIB_NAME}.a"
done

rm -rf "../target/${XCFRAMEWORK_NAME}.xcframework"

XCFRAMEWORK_LIBS=(
    "../target/aarch64-apple-ios/release/${LIB_NAME}.a"
    "../target/ios-simulator/release/${LIB_NAME}.a"
    "../target/macos/release/${LIB_NAME}.a"
    "../target/ios-macabi/release/${LIB_NAME}.a"
    "../target/aarch64-apple-tvos/release/${LIB_NAME}.a"
    "../target/aarch64-apple-visionos/release/${LIB_NAME}.a"
    "../target/aarch64-apple-tvos-sim/release/${LIB_NAME}.a"
    "../target/aarch64-apple-visionos-sim/release/${LIB_NAME}.a"
)

XCFRAMEWORK_ARGS=()
for lib in "${XCFRAMEWORK_LIBS[@]}"; do
    XCFRAMEWORK_ARGS+=(-library "$lib" -headers ./generated/swift)
done

xcodebuild -create-xcframework \
    "${XCFRAMEWORK_ARGS[@]}" \
    -output "../target/${XCFRAMEWORK_NAME}.xcframework"
