#!/bin/sh
# Build the room surface against released WebRTC prebuilts, serially, without workspace edits.
# Run from the Rust workspace. Install the two rustup targets first; outputs use CARGO_TARGET_DIR.
set -eu
: "${IPHONEOS_DEPLOYMENT_TARGET:=14.0}"
export IPHONEOS_DEPLOYMENT_TARGET
if [ -n "${LK_CUSTOM_WEBRTC:-}" ]; then
    echo 'Unset LK_CUSTOM_WEBRTC: this build verifies released WebRTC prebuilts.' >&2
    exit 1
fi
for target in aarch64-apple-ios aarch64-apple-ios-sim; do
    cargo build -j 2 -p livekit-ffi --target "$target" --no-default-features \
        --features room-apis,rustls-tls-webpki-roots
done
