---
livekit-ffi: minor
---

Rename the published UniFFI packages to an FFI identity.

Following the `livekit-uniffi` merge, the packages built from
`livekit-ffi/core-modules` are named after the crate that now produces them. No
compatibility aliases are published.

| | was | now |
|---|---|---|
| Swift package / product | `LiveKitUniFFI` | `LiveKitFFI` |
| Swift FFI module | `RustLiveKitUniFFI` | `RustLiveKitFFI` |
| Maven artifact | `io.livekit:livekit-uniffi-android` | `io.livekit:livekit-ffi-android` |
| Kotlin package | `io.livekit.uniffi` | `io.livekit.ffi` |
| pub.dev package | `livekit_uniffi` | `livekit_ffi` |
| UniFFI namespace | `livekit_uniffi` | `livekit_ffi` |

**Breaking for the Swift, Android and Flutter SDKs.** Imports, dependency
coordinates and the generated module names all change together.

The Swift xcframework continues to be hosted at
`livekit/livekit-uniffi-xcframework`; only the package and product names inside
it change. Versions continue from the merged crate, so no published version
goes backwards within a name.
