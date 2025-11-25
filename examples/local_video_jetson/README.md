Local video capture for Jetson (CPU path, dmabuf-ready)

This example provides two binaries:
- mipi: Captures from MIPI using nvarguscamerasrc (GStreamer), publishes to LiveKit.
- usb: Captures from a V4L2 USB device using v4l2src (GStreamer), publishes to LiveKit.

Both binaries currently use CPU memory (NV12) for compatibility. A dmabuf zero-copy path can be enabled later once the native bridge is implemented.

Run:
- Environment:
  - LIVEKIT_URL, LIVEKIT_API_KEY, LIVEKIT_API_SECRET
  
- MIPI (e.g., 1280x720@30):
  cargo run -p local_video_jetson --bin mipi -- --width 1280 --height 720 --fps 30 --room-name video-room --identity jetson-mipi

- USB device (default /dev/video0):
  cargo run -p local_video_jetson --bin usb -- --device /dev/video0 --width 1280 --height 720 --fps 30 --room-name video-room --identity jetson-usb

Flags:
- --max-bitrate <bps> (optional)
- --h265 (optional; will fallback to H.264 if unsupported)


