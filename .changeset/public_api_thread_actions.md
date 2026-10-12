---
livekit-api: patch
---

Public API tokens minted from an API key and secret now grant every thread action (list, create, read, write, delete) across the project. Before, they carried only `all`, so creating or reading a thread was refused.
