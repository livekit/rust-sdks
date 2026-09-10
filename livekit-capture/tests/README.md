# Integration Tests

Integration tests verify capture sources against real backends, so they are
not enabled by default. Each source has an internal test feature named
`__test-source-<module>` that builds and runs its tests in
`tests/source_<module>_test.rs`. Tests exercise the source directly through
its public API (construction and `next_access_unit`); they do not publish to
an RTC source and need no LiveKit server.

Some sources can only be tested on hosts that provide their backend (for
example, a future `__test-source-device` needs a capture device), so each
section below documents its prerequisites.

## RTSP (`__test-source-rtsp`)

Each test starts an in-process GStreamer RTSP server on an ephemeral
localhost port and streams real encoded video to the source. The system
GStreamer installation must include the RTSP server library, the x264,
x265, VP8/VP9, and AV1 encoder plugins, and the RTP payloaders from
gst-plugins-rs (`rtpav1pay`):

- macOS: `brew install gstreamer`
- Debian/Ubuntu: `apt install libgstrtspserver-1.0-dev
  gstreamer1.0-plugins-good gstreamer1.0-plugins-bad
  gstreamer1.0-plugins-ugly` (in addition to the base development packages)

```sh
cargo test -p livekit-capture --features __test-source-rtsp --test source_rtsp_test
```

The RTSPS tests generate a self-signed certificate at run time (via
`rcgen`), so TLS needs no host setup beyond the GStreamer packages above.

## Logging

Tests use the [`test-log`](https://crates.io/crates/test-log) `#[test]`
attribute, so `log::` output from the sources and the test helpers is
recorded per test. The standard test harness only prints it for *failing*
tests; to see it for passing tests, disable output capture:

```sh
cargo test -p livekit-capture --features __test-source-rtsp --test source_rtsp_test -- --nocapture
```

That shows info-level logs (stream setup, discovered settings). For the
per-access-unit trace, raise the filter:

```sh
RUST_LOG=debug cargo test -p livekit-capture --features __test-source-rtsp --test source_rtsp_test -- --nocapture
```
