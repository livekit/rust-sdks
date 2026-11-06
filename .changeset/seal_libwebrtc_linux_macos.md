---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
---

On linux and macOS, link `libwebrtc.a` sealed: partially linked with the bridge into one object whose only global symbols are its cxx entry points. The FFmpeg, BoringSSL, opus, libvpx, dav1d, libaom and (on linux) libc++ it bundles no longer serve an application that links its own, nor take its place inside libwebrtc. macOS applications no longer need `-ObjC`, and the linux build no longer prefixes the bundled BoringSSL and FFmpeg.
