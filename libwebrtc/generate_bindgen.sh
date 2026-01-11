bindgen libwebrtc/src/livekit_rtc/include/capi.h \
    --output src/sys/ffi.rs \
    --allowlist-type "lk.*" \
    --allowlist-function "lk.*" \
    --default-enum-style rust

bindgen libwebrtc/src/livekit_rtc/include/yuv_helper.h \
    --output src/sys/yuv_helper.rs \
    --allowlist-type "lk.*" \
    --allowlist-function "lk.*" \
    --default-enum-style rust