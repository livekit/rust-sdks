# gstreamer command
```bash
gst-launch-1.0 -v \
  avfvideosrc device-index=0 do-timestamp=true ! \
  video/x-raw,width=800,height=448,framerate=30/1,format=NV12 ! \
  queue max-size-buffers=1 max-size-time=0 max-size-bytes=0 leaky=downstream ! \
  videoconvert ! \
  video/x-raw,format=I420 ! \
  x264enc tune=zerolatency speed-preset=ultrafast bitrate=2500 key-int-max=30 \
    bframes=0 rc-lookahead=0 aud=true sliced-threads=true ! \
  h264parse config-interval=-1 ! \
  video/x-h264,stream-format=byte-stream,alignment=au,profile=baseline ! \
  queue max-size-buffers=1 max-size-time=0 max-size-bytes=0 leaky=downstream ! \
  tcpserversink host=0.0.0.0 port=5005 sync=false async=false
```

# rust command
```bash
  RUST_LOG=info cargo run -p encoded_video_ingest --bin sender -- \
  --url ws://localhost:7880 \
  --api-key devkey \
  --api-secret secret \
  --tcp-host 127.0.0.1 --tcp-port 5005 \
  --width 800 --height 448 --max-framerate 30 \
  --codec h264 \
  --room encoded-video-demo --identity encoded-sender
```

# cli baseline
```bash
lk room join --identity encoder-sender --api-key "devkey" \
  --api-secret "secret" \
  --url "ws://localhost:7880" --publish h264://127.0.0.1:5005 encoded-video-demo
```

gst-launch-1.0 -v videotestsrc is-live=true ! \
  video/x-raw,width=800,height=448,framerate=30/1,format=I420 ! \
  x264enc tune=zerolatency speed-preset=ultrafast bitrate=2500 key-int-max=30 \
  bframes=0 byte-stream=true aud=true ! \
  h264parse config-interval=-1 ! \
  video/x-h264,stream-format=byte-stream,alignment=au,profile=baseline ! \
  tcpserversink host=127.0.0.1 port=5005 sync=false async=false