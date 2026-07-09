---
webrtc-sys: patch
---

Fix `PeerContext` forward-declaration in `jsep.h` from `class` to `struct` to match the cxx bridge definition, resolving LNK2019 linker errors on windows-msvc - #1154