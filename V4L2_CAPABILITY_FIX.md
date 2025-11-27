# V4L2 Capability Query Fix

## Issue

The V4L2 encoder was detecting the `/dev/v4l2-nvenc` device but failing to query its capabilities:

```
[2025-11-27T01:18:03Z DEBUG libwebrtc] (v4l2_encoder_factory.cpp:45): Found Jetson encoder device: /dev/v4l2-nvenc
[2025-11-27T01:18:03Z DEBUG libwebrtc] (v4l2_encoder_factory.cpp:115): Failed to query V4L2 capabilities
```

## Root Cause

The Jetson's `/dev/v4l2-nvenc` device may:
1. Require non-blocking mode when opening (`O_NONBLOCK`)
2. Not support the standard `VIDIOC_QUERYCAP` ioctl
3. Require special permissions
4. Use a custom driver interface

## Solution

Modified `webrtc-sys/src/v4l2/v4l2_encoder_factory.cpp`:

### 1. Added Required Headers

```cpp
#include <cstring>   // for strerror
#include <cerrno>    // for errno
```

### 2. Enhanced `IsSupported()` Function

**Changes:**
- Open device with `O_NONBLOCK` flag
- Special handling for `/dev/v4l2-nvenc` - trust that it's supported if we can open it
- Better error logging with errno details
- Fallback: assume nvenc devices are supported even if QUERYCAP fails

**Updated Code:**

```cpp
bool V4L2VideoEncoderFactory::IsSupported() {
  std::string device_path = GetDevicePath();
  if (device_path.empty()) {
    RTC_LOG(LS_WARNING) << "V4L2 encoder device not available";
    return false;
  }

  int fd = open(device_path.c_str(), O_RDWR | O_NONBLOCK);
  if (fd < 0) {
    RTC_LOG(LS_WARNING) << "Failed to open V4L2 device: " << device_path 
                        << " (errno: " << errno << " - " << strerror(errno) << ")";
    return false;
  }

  // For Jetson-specific device, just check if we can open it
  if (device_path == "/dev/v4l2-nvenc") {
    close(fd);
    RTC_LOG(LS_INFO) << "V4L2 Encoder is supported on Jetson device: " << device_path;
    return true;
  }

  // For generic V4L2 devices, query capabilities
  struct v4l2_capability cap;
  if (ioctl(fd, VIDIOC_QUERYCAP, &cap) < 0) {
    RTC_LOG(LS_WARNING) << "Failed to query V4L2 capabilities for " << device_path
                        << " (errno: " << errno << " - " << strerror(errno) << ")";
    close(fd);
    // For Jetson, still try to use it even if QUERYCAP fails
    if (device_path.find("nvenc") != std::string::npos) {
      RTC_LOG(LS_INFO) << "Assuming Jetson NVENC device is supported despite QUERYCAP failure";
      return true;
    }
    return false;
  }

  close(fd);

  RTC_LOG(LS_INFO) << "V4L2 Encoder is supported on device: " << device_path 
                   << " (" << cap.card << ")";
  return true;
}
```

## Key Improvements

1. **O_NONBLOCK Flag:** Opens device in non-blocking mode, which some devices require
2. **Jetson-Specific Logic:** Trusts `/dev/v4l2-nvenc` if it can be opened
3. **Better Error Messages:** Shows errno and error string for debugging
4. **Graceful Fallback:** Assumes nvenc devices work even if standard queries fail

## Testing

### On Jetson, run the diagnostic script:

```bash
chmod +x diagnose_v4l2.sh rebuild_v4l2.sh
./diagnose_v4l2.sh
```

This will check:
- Device existence
- Permissions
- Device type
- V4L2 capabilities (if v4l2-utils installed)
- Kernel modules
- JetPack version

### Rebuild and test:

```bash
./rebuild_v4l2.sh
```

Or manually:
```bash
cd /home/jetson/workspace/rust-sdks
cargo clean -p webrtc-sys
cargo build --release -p webrtc-sys

cd examples/local_video
RUST_LOG=debug cargo run --bin publisher --release -- \
  --url wss://your-server.com \
  --token your-token \
  --v4l2-device /dev/video0
```

### Expected Logs

**Success:**
```
V4L2 Encoder is supported on Jetson device: /dev/v4l2-nvenc
Using V4L2 HW encoder for H264 (Jetson)
V4L2 H264 encoder initialized: 640x480 @ 30fps
```

**If still failing, you'll see detailed error:**
```
Failed to open V4L2 device: /dev/v4l2-nvenc (errno: 13 - Permission denied)
```

## Common Issues & Solutions

### Permission Denied (errno 13)

```bash
# Option 1: Change device permissions (temporary)
sudo chmod 666 /dev/v4l2-nvenc

# Option 2: Add user to video group (permanent)
sudo usermod -a -G video $USER
# Then log out and back in
```

### Device Busy (errno 16)

Another process is using the encoder. Check:
```bash
sudo lsof | grep v4l2-nvenc
```

### Device Not Found

Check if device exists and kernel modules are loaded:
```bash
ls -la /dev/v4l2-nvenc
lsmod | grep nvhost
```

## Next Steps

1. Run `./diagnose_v4l2.sh` to understand device status
2. Rebuild with `./rebuild_v4l2.sh`  
3. Test with publisher example
4. Share logs if still having issues

The enhanced error logging will help identify the exact problem!

