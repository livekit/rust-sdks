# V4L2 HEVC Pixel Format Fix

## Issue
Build error when compiling V4L2 H.265 encoder implementation:
```
error: 'V4L2_PIX_FMT_H265' was not declared in this scope; did you mean 'V4L2_PIX_FMT_H263'?
```

## Root Cause
The constant `V4L2_PIX_FMT_H265` does not exist in the V4L2 API. The correct pixel format constant for H.265/HEVC encoding is `V4L2_PIX_FMT_HEVC`.

## Changes Made

### Files Modified
1. **webrtc-sys/src/v4l2/v4l2_h265_encoder_impl.cpp**
   - Line 298: Changed `V4L2_PIX_FMT_H265` to `V4L2_PIX_FMT_HEVC`

2. **webrtc-sys/src/v4l2/v4l2_encoder_factory.cpp**
   - Line 78: Changed `V4L2_PIX_FMT_H265` to `V4L2_PIX_FMT_HEVC`

## Verification
After this fix, the `webrtc-sys` crate builds successfully without errors related to V4L2 H.265 encoding.

## Technical Note
`V4L2_PIX_FMT_HEVC` is the standard V4L2 fourcc code for HEVC (High Efficiency Video Coding), which is the formal name for the H.265 video codec. This constant is defined in `linux/videodev2.h` and is supported by the NVIDIA Jetson V4L2 encoder hardware.

