---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

E2EE: setting a key for the local participant with `KeyProvider::set_key` (or a shared key with `set_shared_key`) now also makes its published tracks encrypt with that key's index, as in the other LiveKit SDKs. Before, their frame cryptors stayed on the index they started at until `FrameCryptor::set_key_index` was called on each of them, so a rotated key was silently not used. Tracks published later start on the current index, and neither media nor data tracks take the index of a key that was set for another participant. #1676
