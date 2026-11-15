---
livekit-ffi: patch
---

Reintroduce the `@livekit/uniffi` node package for the `core-modules` surface, built with uniffi-bindgen-react-native's NAPI backend: `@livekit/uniffi` carries the TypeScript and resolves a per-platform `@livekit/uniffi-<triple>` sibling at load time, so nothing is downloaded during install. The packages are built and tested but not published yet.
