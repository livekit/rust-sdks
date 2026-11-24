#!/bin/sh
# Assembles the LiveKitFFI Swift package for macos-arm64 from a `room-apis` cdylib.
#
# The layout is the layout of livekit/livekit-ffi-xcframework:
#   Package.swift, Sources/LiveKitFFI/*.swift, RustLiveKitFFI.xcframework
#
# Usage: package-room-macos.sh <rust worktree> <liblivekit_ffi.dylib> <output directory>
#
# ponytail: one slice, assembled by hand from the host cdylib. Use
# `cargo make swift-package` (cargo-swift) when more than one Apple slice is necessary.
set -eu
WORKTREE=$1
DYLIB=$2
OUT=$3
export PATH="/usr/bin:$PATH"

GEN=$(mktemp -d)
trap 'rm -rf "$GEN"' EXIT

# Library mode reads the UniFFI metadata from the cdylib and `uniffi.toml` from the crate.
(cd "$WORKTREE" && cargo run -q -p bindgens --bin uniffi-bindgen -- generate \
    --language swift --out-dir "$GEN/swift" --library "$DYLIB")

rm -rf "$OUT"
mkdir -p "$OUT/Sources/LiveKitFFI"
cp "$GEN"/swift/*.swift "$OUT/Sources/LiveKitFFI/"
# Dependency components name their own C modules. This package merges all headers into
# RustLiveKitFFI, matching cargo-swift's multi-component packaging.
for binding in "$OUT"/Sources/LiveKitFFI/*.swift; do
    sed -i '' -e 's/canImport(RustLiveKitCommon)/canImport(RustLiveKitFFI)/' \
        -e 's/import RustLiveKitCommon/import RustLiveKitFFI/' \
        -e 's/canImport(RustLiveKitDataTrack)/canImport(RustLiveKitFFI)/' \
        -e 's/import RustLiveKitDataTrack/import RustLiveKitFFI/' "$binding"
done

FRAMEWORK="$GEN/RustLiveKitFFI.framework"
VERSION="$FRAMEWORK/Versions/A"
mkdir -p "$VERSION/Headers" "$VERSION/Modules" "$VERSION/Resources"
cp "$GEN"/swift/*.h "$VERSION/Headers/"
{
    echo "framework module RustLiveKitFFI {"
    for header in "$VERSION"/Headers/*.h; do
        echo "    header \"$(basename "$header")\""
    done
    echo "    export *"
    echo "}"
} > "$VERSION/Modules/module.modulemap"
cp "$DYLIB" "$VERSION/RustLiveKitFFI"
install_name_tool -id @rpath/RustLiveKitFFI.framework/Versions/A/RustLiveKitFFI "$VERSION/RustLiveKitFFI"
cp "$WORKTREE/livekit-ffi/support/swift/PrivacyInfo.xcprivacy" "$VERSION/Resources/"
cat > "$VERSION/Resources/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>RustLiveKitFFI</string>
    <key>CFBundleIdentifier</key>
    <string>io.livekit.research.RustLiveKitFFI</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>RustLiveKitFFI</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>CFBundleSupportedPlatforms</key>
    <array>
        <string>MacOSX</string>
    </array>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
</dict>
</plist>
PLIST
ln -s A "$FRAMEWORK/Versions/Current"
for name in RustLiveKitFFI Headers Modules Resources; do
    ln -s "Versions/Current/$name" "$FRAMEWORK/$name"
done
codesign --force --sign - "$FRAMEWORK"
codesign --verify --deep --strict "$FRAMEWORK"
xcodebuild -create-xcframework -framework "$FRAMEWORK" -output "$OUT/RustLiveKitFFI.xcframework"
# Subsequent copies must preserve the versioned framework symlinks.
codesign --verify --deep --strict \
    "$OUT/RustLiveKitFFI.xcframework/macos-arm64/RustLiveKitFFI.framework"

# The repository templates, development profile: a local path to the xcframework.
for manifest in Package.swift Package@swift-6.2.swift; do
    SPM_NAME=LiveKitFFI CARGO_MAKE_PROFILE=development tera --env-only \
        -t "$WORKTREE/livekit-ffi/support/swift/$manifest.tera" -o "$OUT/$manifest"
done
cp "$WORKTREE/LICENSE" "$OUT/LICENSE"
