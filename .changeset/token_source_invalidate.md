---
livekit-token-source: minor
---

Add `TokenSourceConfigurable::invalidate`, a default no-op that `TokenSourceCached` overrides to clear its store and forward to the source it wraps (so does the inherent `TokenSourceCached::invalidate` now).
