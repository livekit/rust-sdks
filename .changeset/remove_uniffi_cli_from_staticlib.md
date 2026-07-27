---
livekit-uniffi: patch
---

Gate the uniffi `cli` feature (clap + bindgen backends) behind an opt-in feature so it is no longer compiled into shipped library builds, shrinking the static archive by ~43 MiB. The dynamic library is byte-unchanged. - #1275 (@jhugman)
