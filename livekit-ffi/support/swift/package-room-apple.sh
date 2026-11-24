#!/bin/sh
# Assemble already-built, metadata-matched frameworks; never builds native code.
# Usage: package-room-apple.sh <mac package> <device.framework> <sim.framework> <new output dir>
set -eu
[ "$#" -eq 4 ] || { echo "usage: $0 <mac package> <device.framework> <sim.framework> <new output dir>" >&2; exit 1; }
PACKAGE=$(cd -P "$1" && pwd -P)
DEVICE=$(cd -P "$2" && pwd -P)
SIM=$(cd -P "$3" && pwd -P)
MAC=$(cd -P "$PACKAGE/RustLiveKitFFI.xcframework/macos-arm64/RustLiveKitFFI.framework" && pwd -P)
OUT=$(cd -P "$(dirname "$4")" && pwd -P)/$(basename "$4")
for input in "$PACKAGE" "$MAC" "$DEVICE" "$SIM"; do
    case "$OUT" in "$input"|"$input"/*) echo 'output must be outside all inputs' >&2; exit 1;; esac
done
for manifest in "$PACKAGE/Package.swift" "$PACKAGE/Package@swift-6.2.swift"; do
    [ "$(grep -Ec '^[[:space:]]*\.iOS\(\.v(13|14)\),$' "$manifest")" -eq 1 ]
    [ "$(grep -Ec '^[[:space:]]*\.macOS\(\.v(10_15|11)\),$' "$manifest")" -eq 1 ]
done
# Header checks do not replace the prerequisite of verified full binary UniFFI metadata.
for slice in "$DEVICE" "$SIM"; do
    for file in "$MAC"/Headers/*.h "$MAC/Modules/module.modulemap"; do
        cmp "$file" "$slice/${file#"$MAC"/}"
    done
done
mkdir "$OUT" # Refuse to overwrite an existing package.
VERIFY_DIR=$(mktemp -d)
trap 'rm -rf "$VERIFY_DIR"' EXIT
PKG="$OUT/livekit-ffi-xcframework"

# ditto preserves Versions/Current and the macOS framework's top-level symlinks.
ditto "$PACKAGE" "$PKG"
rm -rf "$PKG/RustLiveKitFFI.xcframework"
xcodebuild -create-xcframework \
    -framework "$MAC" \
    -framework "$DEVICE" -framework "$SIM" -output "$PKG/RustLiveKitFFI.xcframework"
for manifest in "$PKG/Package.swift" "$PKG/Package@swift-6.2.swift"; do
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
