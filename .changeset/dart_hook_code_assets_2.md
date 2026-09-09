---
livekit-uniffi: patch
---

# Allow code_assets 2.x in the Dart package and stop re-running its build hook

The `livekit_uniffi` Dart package capped `code_assets` below 2.0.0, which has since
shipped. Its only breaking change (equality on `OS` and `Architecture`) does not
affect the hook, and the cap would make the package unresolvable next to any
dependency that already requires 2.x. The constraint now allows it, and the hook
was verified against `code_assets 2.0.0` / `hooks 2.2.0`.

The hook also registered the downloaded library as a dependency. Dependencies are
inputs, so the hooks runner saw a file modified during the build and re-ran the
hook, and the download, once on every fresh build. The registration is removed.

The hook also wrote every target's library to the same shared path. A universal
macOS build runs the hook once per architecture and then merges the results with
`lipo`, which failed because the second download had overwritten the first. Each
target now gets its own subdirectory.
