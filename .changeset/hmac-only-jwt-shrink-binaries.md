---
livekit-api: patch
---

# Shrink binaries with an HMAC-only JWT crypto provider

`livekit-api` now installs a minimal in-crate HMAC `CryptoProvider`
(HS256/384/512) instead of jsonwebtoken's `rust_crypto` backend, dropping the
unused RSA/EC/EdDSA algorithms. LiveKit access tokens are HS256, so there is no
public API or behavior change. This trims ~200 KiB of unreachable crypto code,
shrinking the shipped binaries ~25–29% — the `RustLiveKitUniFFI` iOS framework
drops from ~900 KiB to ~672 KiB (arm64) and the Android arm64-v8a `.so` from
~1.13 MiB to ~816 KiB. `livekit-ffi` and `livekit` benefit too.
