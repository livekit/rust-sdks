#!/bin/bash

# Diagnostic script to check V4L2 device status

echo "=========================================="
echo "V4L2 NVENC Device Diagnostics"
echo "=========================================="
echo ""

DEVICE="/dev/v4l2-nvenc"

echo "1. Checking if device exists..."
if [ -e "$DEVICE" ]; then
    echo "   ✅ $DEVICE exists"
    ls -la "$DEVICE"
else
    echo "   ❌ $DEVICE does NOT exist"
    echo ""
    echo "   Available video devices:"
    ls -la /dev/video* 2>/dev/null || echo "   No /dev/video* devices found"
    exit 1
fi

echo ""
echo "2. Checking device permissions..."
if [ -r "$DEVICE" ] && [ -w "$DEVICE" ]; then
    echo "   ✅ Device is readable and writable by current user"
else
    echo "   ⚠️  Device may not be accessible"
    echo "   Try: sudo chmod 666 $DEVICE"
    echo "   Or add your user to the 'video' group:"
    echo "   sudo usermod -a -G video $USER"
fi

echo ""
echo "3. Checking device type..."
if [ -c "$DEVICE" ]; then
    echo "   ✅ Device is a character device (correct)"
else
    echo "   ⚠️  Device is not a character device"
fi

echo ""
echo "4. Checking if v4l2-ctl is available..."
if command -v v4l2-ctl &> /dev/null; then
    echo "   ✅ v4l2-ctl is installed"
    echo ""
    echo "5. Querying device capabilities..."
    sudo v4l2-ctl -d "$DEVICE" --all 2>&1 | head -30
    
    echo ""
    echo "6. Querying supported formats..."
    sudo v4l2-ctl -d "$DEVICE" --list-formats-ext 2>&1 | head -50
else
    echo "   ⚠️  v4l2-ctl is not installed"
    echo "   Install with: sudo apt-get install v4l-utils"
fi

echo ""
echo "7. Checking kernel modules..."
echo "   Loaded V4L2 modules:"
lsmod | grep -E "(v4l2|nvhost|tegra)" || echo "   No V4L2 modules found"

echo ""
echo "8. Checking JetPack version..."
if [ -f /etc/nv_tegra_release ]; then
    echo "   JetPack info:"
    cat /etc/nv_tegra_release
else
    echo "   /etc/nv_tegra_release not found"
fi

echo ""
echo "9. Testing device open (requires sudo)..."
if sudo sh -c "exec 3<>$DEVICE && exec 3>&-" 2>/dev/null; then
    echo "   ✅ Device can be opened"
else
    echo "   ❌ Failed to open device"
    echo "   Error: $?"
fi

echo ""
echo "=========================================="
echo "Diagnostic Complete"
echo "=========================================="

