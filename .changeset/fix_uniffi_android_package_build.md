---
livekit-uniffi: patch
---

Fix the Android AAR build. Two cargo-make bugs kept `cargo make --profile release android-package` from ever producing a release artifact: the per-arch tasks' `env = { TARGET = ... }` replaced (rather than merged) the parent env map, dropping the `--release` flag, and the `TARGET` they set leaked into the Kotlin bindgen's host build, which then cross-compiled with the host linker. Also raise the Swift and Android size budgets to match the binaries as they stand since the data-track UniFFI surface landed, and check out the released tag rather than the dispatch ref when building the wrapper packages.
