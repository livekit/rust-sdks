#!/bin/bash

# Rebuild script for testing the capability query fix

echo "=========================================="
echo "Rebuilding with V4L2 capability fix..."
echo "=========================================="
echo ""

cd /home/jetson/workspace/rust-sdks

# Clean previous build to ensure fresh compilation
cargo clean -p webrtc-sys

# Rebuild webrtc-sys
echo "Building webrtc-sys..."
cargo build --release -p webrtc-sys 2>&1 | grep -E "(Compiling webrtc-sys|Building with V4L2|warning: webrtc-sys)"

if [ $? -eq 0 ]; then
    echo ""
    echo "✅ webrtc-sys built successfully"
    echo ""
    echo "Now testing the encoder..."
    echo ""
    
    # Try building the publisher
    cd examples/local_video
    cargo build --bin publisher --release 2>&1 | tail -20
    
    echo ""
    echo "=========================================="
    echo "Build complete!"
    echo "=========================================="
    echo ""
    echo "To test the encoder, run:"
    echo ""
    echo "  cd examples/local_video"
    echo "  RUST_LOG=debug cargo run --bin publisher --release -- \\"
    echo "    --url wss://your-server.com \\"
    echo "    --token your-token \\"
    echo "    --v4l2-device /dev/video0"
    echo ""
    echo "Look for log messages like:"
    echo "  'V4L2 Encoder is supported on Jetson device: /dev/v4l2-nvenc'"
    echo "  'Using V4L2 HW encoder for H264 (Jetson)'"
    echo ""
else
    echo ""
    echo "❌ Build failed"
    echo ""
    exit 1
fi

