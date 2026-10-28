# Outgoing zero-input encoder scheduling

This branch contains a native WebRTC scheduling candidate in
`webrtc-sys/libwebrtc/patches/silence_encoder_scheduling.patch`. The native build
scripts apply it automatically. **Rebuilding just the Rust/Python SDK against
the existing prebuilt WebRTC tag does not enable this change.**

The change keeps the existing buffered source's 10-ms clock and every encoder
call. For exact-zero PCM, it may hold one frame until the next input frame when
the codec says no work is required before that point. New nonzero input and
ordered controls flush pending input. Opus uses its current packet boundary;
RED forwards the inner capability. Unsupported encoders, 10-ms Opus packets and
Opus audio-network adaptation retain ordinary processing.

It does not freeze codec state, stop/start the stream, change Opus complexity,
change RTP timestamps or sleep for an inferred DTX refresh interval. Source,
resampler, application callbacks and transport timers retain their clocks.

## Build a usable RTC library

The native interface adds an encoder capability, so all native encoders and
callers must be rebuilt together. Use the branch's normal native build script,
then point the FFI build at its matching artifact:

```sh
git submodule update --init --recursive
cd webrtc-sys/libwebrtc
./build_linux.sh --arch x64 --profile release
cd ../..
LK_CUSTOM_WEBRTC="$PWD/webrtc-sys/libwebrtc/linux-x64-release" \
  cargo build --release -p livekit-ffi
```

For macOS arm64 use `build_macos.sh --arch arm64 --profile release` and the
`mac-arm64-release` artifact instead. The native scripts download their normal
WebRTC toolchain/dependencies. Run them from `webrtc-sys/libwebrtc`.

Use the resulting library with an existing Python RTC installation:

```sh
LIVEKIT_LIB_PATH="$PWD/target/release/liblivekit_ffi.so" python your_agent.py
```

On macOS the filename is `liblivekit_ffi.dylib`. Keep buffered audio sources;
this patch does not repair the missing media-time semantics of unbuffered input
gaps. No Python API change or encoder-quality setting is required.

The `Audio silence native build` workflow builds the complete Linux x64 native
library and FFI, runs the standalone tests, and uploads
`livekit-rtc-silence-linux-x64`. It does not publish a release. A new WebRTC
artifact/tag pin is still required before this can become the SDK's default.

## Standalone checks

Use the rebuilt artifact, not an older downloaded archive:

```sh
python webrtc-sys/test/outgoing_silence/run.py --case factory --webrtc-dir /path/to/artifact
python webrtc-sys/test/outgoing_silence/run.py --case queue --webrtc-dir /path/to/artifact
python webrtc-sys/test/outgoing_silence/run.py --case runtime --webrtc-dir /path/to/artifact
```

On Linux pass `--cxx` pointing to the native checkout's
`src/third_party/llvm-build/Release+Asserts/bin/clang++`, or a sufficiently recent
clang. The runner uses the artifact's Chromium libc++ headers and ABI defines,
matching `webrtc-sys/build.rs`. On macOS Xcode's `clang++` is sufficient. For the
queue test, `--sanitize` enables AddressSanitizer and UndefinedBehaviorSanitizer.
Explicit runtime checks remain enabled with `NDEBUG`.

The conformance harness uses a simulated clock and generated PCM. Its trace
includes 30- and 60-second gaps, changing bitrate/loss controls and a reset.
Cases cover mono/stereo, 24/48 kbps, DTX on/off, packetization from 10 to 120 ms,
queue backlog, and RED at 20 ms. It compares completed nonempty and DTX-empty
outputs, payload bytes, timestamps, logical emission frames and RED metadata.
The queue test covers speech/control preemption, stale budget publication,
unsupported-capability caching, concurrent admission and joined teardown.

See [DESIGN.md](DESIGN.md) for the ownership and ordering contract, and
[VALIDATION.md](VALIDATION.md) for observed results and remaining limitations.

## CPU comparison

`rtc_cpu.py` is an optional integration benchmark requiring `livekit`,
`livekit-api`, `psutil` and the normal `LIVEKIT_*` credentials. It measures
simultaneous baseline/candidate agent processes, each with four fresh RTC rooms.
Incoming generated audio stays active; outgoing audio is either active or the
buffered source's synthetic zeros. Codec formats must match before and after
measurement, otherwise the trial fails and must be excluded.

```sh
python webrtc-sys/test/outgoing_silence/rtc_cpu.py \
  --baseline /path/to/baseline-library --candidate /path/to/candidate-library \
  --out /tmp/silence-cpu --sessions 4 --repeats 3 --duration 15 --parallel 2
```

The default benchmark explicitly uses dual peer connections, DTX and RED. Use
`--single-pc 1` to test single-PC mode, retaining the codec-matching checks.
Compare libraries built from the same SDK revision and compiler/profile,
changing only the native scheduling implementation. Fewer queue submissions
alone are not evidence of lower process CPU or higher session capacity.
