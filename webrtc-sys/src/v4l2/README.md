# V4L2 Hardware Video Encoder for NVIDIA Jetson

This directory contains the V4L2 M2M (Memory-to-Memory) hardware video encoder implementation for NVIDIA Jetson devices (tested on Jetson Orin NX with JetPack 6).

## Overview

The V4L2 encoder provides hardware-accelerated H.264 and H.265/HEVC encoding on Jetson devices through the Linux V4L2 (Video4Linux2) API. This implementation uses the NVENC hardware encoder available on Jetson platforms without requiring CUDA.

## Features

- **Hardware Acceleration**: Direct access to Jetson's dedicated video encoding hardware
- **Low Latency**: Optimized for real-time video streaming applications
- **Multiple Codecs**: Support for both H.264 (Baseline profile) and H.265/HEVC (Main profile)
- **Standard Buffers**: Uses standard video frame buffers (I420 format)
- **Automatic Detection**: Automatically detects and uses V4L2 encoder devices

## Device Detection

The encoder factory attempts to find the V4L2 encoder device in the following order:

1. `/dev/v4l2-nvenc` - Primary Jetson encoder device
2. `/dev/video0` through `/dev/video3` - Generic V4L2 devices with encoding capability

## Requirements

- NVIDIA Jetson device (Orin NX, Xavier NX, or similar)
- JetPack 6.0 or later
- Linux kernel with V4L2 M2M support
- Access permissions to `/dev/v4l2-nvenc` or `/dev/videoX` devices

## Building

The V4L2 encoder is automatically enabled when building on ARM64 Linux if the Jetson encoder device is detected:

```bash
# Build will automatically detect Jetson device
cargo build --release
```

To verify detection during build:
```bash
cargo build 2>&1 | grep -i jetson
# Should see: "Building with V4L2 encoder support for Jetson"
```

## Usage

### From Rust Code

The encoder is automatically selected when creating a video track on Jetson:

```rust
use livekit::prelude::*;

// The V4L2 encoder will be automatically used on Jetson
let track = LocalVideoTrack::create_video_track("camera", rtc_source);
```

### Command Line Example

```bash
# Use default H.264 encoding
./publisher --camera-index 0

# Use H.265/HEVC encoding (recommended for Jetson)
./publisher --camera-index 0 --h265

# Force software encoder (for debugging)
./publisher --camera-index 0 --software-encoder
```

## Verification

When the V4L2 encoder is active, you should see log messages like:

```
V4L2 device opened successfully: <device name>
V4L2 H264 encoder initialized: 1280x720 @ 30fps, target_bps=2000000 using device /dev/v4l2-nvenc
Using V4L2 HW encoder for H264 (Jetson)
```

## Performance

The V4L2 hardware encoder on Jetson provides:
- **Low CPU Usage**: < 5% CPU for 1080p30 encoding
- **High Quality**: Hardware-optimized encoding quality
- **Low Latency**: Typically < 33ms encode latency
- **Multiple Streams**: Can encode multiple streams simultaneously

## Troubleshooting

### Device Not Found

If the encoder device is not detected:

```bash
# Check if encoder device exists
ls -l /dev/v4l2-nvenc /dev/video*

# Verify V4L2 capabilities
v4l2-ctl --list-devices
v4l2-ctl -d /dev/v4l2-nvenc --list-formats-ext
```

### Permission Denied

If you get permission errors:

```bash
# Add user to video group
sudo usermod -a -G video $USER

# Or run with sudo for testing
sudo ./publisher
```

### Encoder Initialization Failed

If encoder initialization fails:

1. Verify JetPack version: `sudo apt-cache show nvidia-jetpack`
2. Check kernel modules: `lsmod | grep nvenc`
3. Check dmesg for errors: `sudo dmesg | grep -i nvenc`
4. Try different device paths in the factory code

## Future Enhancements

Planned improvements:
- **Zero-copy Path**: Direct DMA buffer support for even lower latency
- **NvBufSurface Integration**: Use Jetson's buffer management API
- **Advanced Controls**: Bitrate adaptation, QP control, etc.
- **Multi-plane Optimization**: Better multi-plane buffer handling

## References

- [NVIDIA Jetson Linux Multimedia API](https://docs.nvidia.com/jetson/l4t-multimedia/)
- [V4L2 Video Encoder Documentation](https://docs.nvidia.com/jetson/l4t-multimedia/group__V4L2Enc.html)
- [V4L2 API Reference](https://www.kernel.org/doc/html/latest/userspace-api/media/v4l/v4l2.html)

## License

Same as the parent project (Apache 2.0).

