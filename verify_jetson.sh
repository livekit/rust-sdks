#!/bin/bash
# Jetson V4L2 Encoder Verification Script
# This script helps verify that the V4L2 encoder is properly set up on Jetson

set -e

echo "=========================================="
echo "Jetson V4L2 Encoder Verification"
echo "=========================================="
echo ""

# Check if running on ARM64
ARCH=$(uname -m)
if [ "$ARCH" != "aarch64" ]; then
    echo "⚠️  Not running on ARM64 architecture (found: $ARCH)"
    echo "   V4L2 encoder is only supported on ARM64 Jetson devices"
    exit 1
fi

echo "✅ Running on ARM64 architecture"
echo ""

# Check for Jetson device files
echo "Checking for Jetson encoder devices..."
if [ -e /dev/v4l2-nvenc ]; then
    echo "✅ Found primary Jetson encoder device: /dev/v4l2-nvenc"
    ls -l /dev/v4l2-nvenc
else
    echo "⚠️  Primary device /dev/v4l2-nvenc not found"
fi
echo ""

# Check for alternative V4L2 devices
echo "Checking for V4L2 video devices..."
if command -v v4l2-ctl &> /dev/null; then
    v4l2-ctl --list-devices | head -20
else
    echo "⚠️  v4l2-ctl not installed. Install with: sudo apt-get install v4l-utils"
fi
echo ""

# Check JetPack version
echo "Checking JetPack version..."
if [ -f /etc/nv_tegra_release ]; then
    cat /etc/nv_tegra_release
elif command -v apt-cache &> /dev/null; then
    apt-cache show nvidia-jetpack 2>/dev/null | grep Version | head -1 || echo "⚠️  JetPack version not detected"
else
    echo "⚠️  Unable to determine JetPack version"
fi
echo ""

# Check permissions
echo "Checking device permissions..."
CURRENT_USER=$(whoami)
if groups $CURRENT_USER | grep -q "\bvideo\b"; then
    echo "✅ User '$CURRENT_USER' is in 'video' group"
else
    echo "⚠️  User '$CURRENT_USER' is NOT in 'video' group"
    echo "   Add user to video group: sudo usermod -a -G video $CURRENT_USER"
    echo "   Then log out and log back in"
fi
echo ""

# Check if build will include V4L2 support
echo "Build configuration check..."
if [ -e /dev/v4l2-nvenc ]; then
    echo "✅ V4L2 encoder will be enabled during build"
    echo "   The build system will detect /dev/v4l2-nvenc and enable USE_V4L2_VIDEO_CODEC"
else
    echo "⚠️  V4L2 encoder may not be enabled during build"
    echo "   Device /dev/v4l2-nvenc not found"
fi
echo ""

# Check for CUDA (should not be required for V4L2)
echo "Checking for CUDA..."
if [ -d /usr/local/cuda ]; then
    echo "ℹ️  CUDA found at /usr/local/cuda"
    echo "   Note: CUDA is NOT required for V4L2 encoder"
else
    echo "ℹ️  CUDA not found (this is OK for V4L2 encoder)"
fi
echo ""

# Summary
echo "=========================================="
echo "Verification Summary"
echo "=========================================="
if [ -e /dev/v4l2-nvenc ] && groups $CURRENT_USER | grep -q "\bvideo\b"; then
    echo "✅ System appears ready for V4L2 encoder"
    echo ""
    echo "Next steps:"
    echo "1. Build the project: cargo build --release"
    echo "2. Check build output for: 'Building with V4L2 encoder support for Jetson'"
    echo "3. Run example: RUST_LOG=info ./target/release/publisher --camera-index 0"
    echo "4. Look for log: 'V4L2 H264 encoder initialized'"
else
    echo "⚠️  System may need configuration"
    echo ""
    echo "Issues detected:"
    if [ ! -e /dev/v4l2-nvenc ]; then
        echo "- Jetson encoder device not found"
    fi
    if ! groups $CURRENT_USER | grep -q "\bvideo\b"; then
        echo "- User not in video group"
    fi
fi
echo ""

