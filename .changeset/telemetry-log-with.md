---
livekit-telemetry: minor
---

Add `global::log_with`, which builds a log record only while a pipeline is installed, and free the instruments that `global::uninstall` and `global::disable` remove outside the installed-pipeline lock, so an instrument destructor can log.
