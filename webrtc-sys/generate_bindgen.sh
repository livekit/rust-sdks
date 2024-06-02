bindgen libwebrtc/src/livekit_rtc/livekit/capi.h \
    --output src/sys/ffi.rs \
    --allowlist-type "lk.*" \
    --allowlist-function "lk.*" \
    --default-enum-style rust
