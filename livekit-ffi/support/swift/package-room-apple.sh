#!/bin/sh
# Assemble already-built, metadata-matched frameworks; never builds native code.
# Usage: package-room-apple.sh <mac package> <device.framework> <sim.framework> <new output dir>
set -eu
[ "$#" -eq 4 ] || { echo "usage: $0 <mac package> <device.framework> <sim.framework> <new output dir>" >&2; exit 1; }
PACKAGE=$(cd "$1" && pwd)
DEVICE=$(cd "$2" && pwd)
SIM=$(cd "$3" && pwd)
mkdir "$4" # Refuse to overwrite an existing package.
OUT=$(cd "$4" && pwd)
VERIFY_DIR=$(mktemp -d)
trap 'rm -rf "$VERIFY_DIR"' EXIT
PKG="$OUT/livekit-ffi-xcframework"

# ditto preserves Versions/Current and the macOS framework's top-level symlinks.
ditto "$PACKAGE" "$PKG"
rm -rf "$PKG/RustLiveKitFFI.xcframework"
xcodebuild -create-xcframework \
    -framework "$PACKAGE/RustLiveKitFFI.xcframework/macos-arm64/RustLiveKitFFI.framework" \
    -framework "$DEVICE" -framework "$SIM" -output "$PKG/RustLiveKitFFI.xcframework"
for manifest in "$PKG/Package.swift" "$PKG/Package@swift-6.2.swift"; do
    # Accept only the known template or already-correct minimums; fail on drift.
    [ "$(grep -Ec '^[[:space:]]*\.iOS\(\.v(13|14)\),$' "$manifest")" -eq 1 ]
    [ "$(grep -Ec '^[[:space:]]*\.macOS\(\.v(10_15|11)\),$' "$manifest")" -eq 1 ]
    sed -i '' -e 's/\.iOS(\.v13)/.iOS(.v14)/' \
        -e 's/\.macOS(\.v10_15)/.macOS(.v11)/' "$manifest"
done
verify() {
    test -L "$1/RustLiveKitFFI.xcframework/macos-arm64/RustLiveKitFFI.framework/Versions/Current"
    for slice in macos-arm64 ios-arm64 ios-arm64-simulator; do
        codesign --verify --deep --strict \
            "$1/RustLiveKitFFI.xcframework/$slice/RustLiveKitFFI.framework"
    done
}
verify "$PKG"
ditto -c -k --sequesterRsrc --keepParent "$PKG" "$OUT/LiveKitFFI.zip"
ditto -x -k "$OUT/LiveKitFFI.zip" "$VERIFY_DIR"
verify "$VERIFY_DIR/livekit-ffi-xcframework"
shasum -a 256 "$OUT/LiveKitFFI.zip"
