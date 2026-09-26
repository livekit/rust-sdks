---
livekit-uniffi: patch
---

# Build the node package with uniffi-bindgen-react-native

The node bindings now come from ubrn's NAPI backend and ship in the layout the
node ecosystem already uses: `@livekit/uniffi` carries the TypeScript and
declares one optional dependency per platform, and `@livekit/uniffi-<triple>`
carries just the native library. The right one is resolved at load time, so
nothing downloads a library during install — the previous package fetched its
cdylib from a GitHub release on first use.

Node packages are not published yet; this lands the build and its tests.
