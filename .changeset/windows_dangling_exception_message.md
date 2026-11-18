---
webrtc-sys: patch
---

Fix an intermittent abort on Windows when a webrtc-sys call reports an error to Rust. The prebuilt libwebrtc is compiled with `_HAS_EXCEPTIONS=0`, and the build script copies that define onto this crate, so MSVC's `std::runtime_error(const std::string&)` kept only a pointer to the temporary message instead of copying it. `what()` then dangled while the exception unwound and cxx read freed memory to build the Rust `Err`. Error messages are now owned by the thrown exception.
