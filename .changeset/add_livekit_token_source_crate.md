---
livekit-token-source: minor
---

Add the `livekit-token-source` crate: token sources for procuring LiveKit credentials, mirroring the JS SDK's `TokenSource` — `literal`, `endpoint` (standard token endpoint format), and `development_token_server`, plus `TokenSourceFixed` / `TokenSourceConfigurable` traits for custom backends. Any configurable source can be wrapped via `.cached()` to reuse credentials until the token expires, with pluggable storage (`TokenSourceStore`) and validation.
