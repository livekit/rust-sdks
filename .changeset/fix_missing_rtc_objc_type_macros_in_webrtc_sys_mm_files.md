---
webrtc-sys: patch
---

# Fix missing RTC_OBJC_TYPE macros in webrtc-sys .mm files

Wrap bare ObjC class references in `RTC_OBJC_TYPE()` in `objc_video_factory.mm` and `objc_video_frame_buffer.mm` to support builds with `rtc_objc_prefix` set.
