---
webrtc-sys: patch
---

# Fix WebRTC build scripts to properly report failures and fix C++ module compilation issues

- Add `set -e` to all build scripts so CI properly reports build failures instead of silently creating empty/broken artifacts
- Re-add `use_clang_modules=false` to macOS, iOS, and Linux build scripts to fix C++ module compilation errors

Without `use_clang_modules=false`, builds fail due to libc++ header incompatibilities (on macOS/iOS with Xcode 26.0) or other C++ module issues, resulting in:
- macOS/iOS: Empty `libwebrtc.a` (~13KB instead of ~700MB)
- Android: Missing `libwebrtc.jar`
- Linux: Incomplete artifacts

The builds appeared successful because the scripts continued after ninja failures, but now with `set -e`, failures will be properly reported.
