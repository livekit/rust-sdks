## 0.1.2 (2026-08-25)

### Features

- Adds optional caching version of the token sources

## 0.1.1 (2026-08-10)

### Features

- Add the `livekit-token-source` crate: token sources for procuring LiveKit credentials, mirroring the JS SDK's `TokenSource` — `literal`, `endpoint` (standard token endpoint format), and `development_token_server`, plus `TokenSourceFixed` / `TokenSourceConfigurable` traits for custom backends.

### Fixes

- Add a TokenSource crate to the Rust SDKs  - #1274 (@MaxHeimbrock)
