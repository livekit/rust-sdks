# Test results — issue #1407

`Room::close()` does not return when the signal link stops delivering data
https://github.com/livekit/rust-sdks/issues/1407

## Environment

- Date: 2026-09-10 19:09 UTC
- Host: Windows 11, x86_64-pc-windows-msvc (linker: VS 2022 Community, MSVC 14.44.35207, Windows SDK 10.0.22621.0)
- Toolchain: rustc 1.97.1 (8bab26f4f 2026-07-14) — the version pinned by the repo's `rust-toolchain.toml`,
  installed for this run and removed afterwards
- Base commit: tests were run at `8a85181c` (Compile all features in ci (#1393)). The branch
  was later rebased onto `42d6bc5a` (Expose pre-encoded video ingest through FFI (#1418)),
  which touches no file under `livekit-signaling/`, so the results carry over unchanged.
- Changed: livekit-signaling/src/signal_stream.rs, livekit-signaling/src/lib.rs

## Summary

| Check | Command | Result |
| --- | --- | --- |
| Unit tests | `cargo test -p livekit-signaling --lib` | **42 passed, 0 failed** |
| Regression proof (pre-fix code) | same, 3 new tests only | **3 failed** — as expected |
| Formatting | `cargo fmt -p livekit-signaling -- --check` | **clean** (exit 0) |
| Lints | `cargo clippy -p livekit-signaling --all-targets` | **exit 0**, no new warnings |
| Feature check | `cargo check -p livekit-signaling --features native --all-targets` | **exit 0** |

## 1. Unit tests, with the fix applied

```
running 42 tests
...
test signal_stream::tests::close_without_notify_returns_on_a_dead_link ... ok
test signal_stream::tests::close_returns_on_a_dead_link ... ok
test signal_stream::tests::dropping_the_stream_stops_its_tasks ... ok

test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```

The whole suite finishes in 0.03s: no test reaches a timeout, so `close()` is
returning promptly rather than being rescued by the `CLOSE_DRAIN_TIMEOUT` backstop.

## 2. Regression proof — the same tests against pre-fix behaviour

To confirm the tests actually pin the bug, the two production changes were
temporarily reverted (read task back to a bare `conn.recv().await` with no
shutdown arm; `close()` back to awaiting both handles unbounded) while keeping
the new tests. All three failed:

```
test signal_stream::tests::close_returns_on_a_dead_link ... FAILED
test signal_stream::tests::close_without_notify_returns_on_a_dead_link ... FAILED
test signal_stream::tests::dropping_the_stream_stops_its_tasks ... FAILED

---- signal_stream::tests::close_returns_on_a_dead_link stdout ----
panicked at livekit-signaling\src\signal_stream.rs:256:9:
SignalStream::close() did not return on a dead link

---- signal_stream::tests::close_without_notify_returns_on_a_dead_link stdout ----
panicked at livekit-signaling\src\signal_stream.rs:268:9:
SignalStream::close(false) did not return on a dead link

---- signal_stream::tests::dropping_the_stream_stops_its_tasks stdout ----
panicked at livekit-signaling\src\signal_stream.rs:281:9:
a dropped stream left its tasks running

test result: FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 39 filtered out; finished in 10.02s
```

`finished in 10.02s` is the tests' own 10s guard expiring — the reproduction the
issue reports. The source file was then restored from a byte-identical backup
(md5 `8889f60fb9a7d1e0b74ea97fff60f3fd`, verified after restore).

## 3. Clippy

`cargo clippy -p livekit-signaling --all-targets` exits 0. `livekit-signaling`
emits 4 lib warnings + 3 test warnings, all pre-existing and none in the changed
code:

| Warning | Location | Mine? |
| --- | --- | --- |
| large size difference between variants | `signal_stream.rs:25` (`InternalMessage` enum) | no — untouched |
| `to_string` in `error!` args | `lib.rs:259` | no |
| `clone` on `u32` | `lib.rs:384` | no |
| too many arguments (8/7) | `lib.rs:816` | no |
| `std::io::Error::other` | `region_url_provider.rs:356` | no |
| `.err().expect()` | `lib.rs:1371` | no |
| field assignment outside initializer | `lib.rs:1440` | no |

## Not covered by this run

- The workspace's other crates were not built. `SignalStream` is `pub(super)`
  and the public signature of `SignalClient::close` is unchanged, so there is no
  downstream API impact — but a full `cargo build` needs the WebRTC prebuilts and
  was not attempted here.
- The end-to-end test the issue reporter offered (a TCP proxy in front of the
  signal WebSocket, holding both sockets open while forwarding nothing, against
  `livekit-server --dev`) was not run. It exercises the real transport rather
  than a mock and would be worth taking them up on.
