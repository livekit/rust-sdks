# Abseil usage

- Default: bundled Abseil via WebRTC’s third_party/abseil-cpp. No system Abseil is required.
- System Abseil is optional and must be configured to match WebRTC’s inline namespace expectations.

Recommended: bundled Abseil (default)
- Build clean with env vars unset:
  - Bash:
    - env -u USE_SYSTEM_ABSEIL -u ABSEIL_ROOT -u ABSEIL_DIR -u ABSEIL_LIB_DIR cargo clean
    - env -u USE_SYSTEM_ABSEIL -u ABSEIL_ROOT -u ABSEIL_DIR -u ABSEIL_LIB_DIR cargo build -vv
- Verify logs:
  - No “Using system Abseil” warning from [`webrtc-sys/build.rs`](webrtc-sys/build.rs:196)
  - No cargo:rustc-link-lib=dylib=absl_* lines (system-only) from [`webrtc-sys/build.rs`](webrtc-sys/build.rs:261)

Using a system Abseil
- You must enable WebRTC’s inline namespace in Abseil:
  - ABSL_OPTION_USE_INLINE_NAMESPACE=1
  - ABSL_OPTION_INLINE_NAMESPACE_NAME=webrtc_absl
- Example CMake build/install:
  - cmake -S abseil-cpp -B build -DABSL_OPTION_USE_INLINE_NAMESPACE=ON -DABSL_OPTION_INLINE_NAMESPACE_NAME=webrtc_absl -DBUILD_SHARED_LIBS=ON -DCMAKE_POSITION_INDEPENDENT_CODE=ON -DCMAKE_INSTALL_PREFIX=/usr/local
  - cmake --build build -j
  - sudo cmake --install build
- Build env for this repo:
  - USE_SYSTEM_ABSEIL=1 ABSEIL_ROOT=/usr/local ABSEIL_LIB_DIR=/usr/local/lib cargo build -vv
- Verify options.h defines (any of these standard locations):
  - /usr/local/include/absl/base/options.h
  - /usr/include/absl/base/options.h
  - /usr/local/absl/base/options.h
  - Should contain:
    - #define ABSL_OPTION_USE_INLINE_NAMESPACE 1
    - #define ABSL_OPTION_INLINE_NAMESPACE_NAME webrtc_absl

Include path priority notes
- The build adds ./include first, then Abseil and WebRTC paths; see [`webrtc-sys/build.rs`](webrtc-sys/build.rs:209) and insertion near [`webrtc-sys/build.rs`](webrtc-sys/build.rs:215).
- Ensure no absl/ headers exist under ./include to avoid shadowing system headers.

Troubleshooting
- If your system Abseil shows inline namespace “head” or USE_INLINE_NAMESPACE=0, expect ODR/link errors. Use bundled Abseil (default) or rebuild system Abseil as above.
- Abseil usage in this repo (examples):
  - Includes: absl/strings/match.h, absl/types/optional.h e.g. [`webrtc-sys/include/livekit/video_decoder_factory.h`](webrtc-sys/include/livekit/video_decoder_factory.h:21), [`webrtc-sys/src/rtp_receiver.cpp`](webrtc-sys/src/rtp_receiver.cpp:22)
  - Build defines for inline namespace: [`webrtc-sys/build.rs`](webrtc-sys/build.rs:248), [`webrtc-sys/build.rs`](webrtc-sys/build.rs:249)