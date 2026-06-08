---
webrtc-sys: patch
libwebrtc: minor
livekit: minor
---

# Make GLib an opt-in dependency

`webrtc-sys` no longer links against `glib-2.0`/`gobject-2.0`/`gio-2.0` by default.

Breaking: Wayland screen sharing now requires the `glib-main-loop` feature on `livekit` (or `libwebrtc`).
