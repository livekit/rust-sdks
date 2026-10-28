# Outgoing-silence encoder experiment

Research only: this adds no runtime sender optimization. The deferred mode uses
packet deadlines learned from a continuous reference run (an **oracle**). A real
codec-owned deadline predictor has not been implemented.

The buffered source in `webrtc-sys/src/audio_track.cpp` supplies a 10-ms zero frame
when its queue cannot supply a full frame. This experiment asks whether those
known-zero frames can be processed later, in order, without changing nonempty
encoder outputs. It also checks the counterexample of skipping zeros while
preserving the input RTP timeline. See [the proposed native contract](DESIGN.md).

## Reproduce on macOS Apple Silicon

Requires Python 3 and Xcode command-line tools. Use the SDK's pinned artifact;
this example matches `WEBRTC_TAG = webrtc-a545de4` at base commit `ede8ac53`.
The archive is a build dependency and should stay outside the repository.

```sh
curl -fL -o /tmp/webrtc-outgoing.zip \
  https://github.com/livekit/rust-sdks/releases/download/webrtc-a545de4/webrtc-mac-arm64-release.zip
unzip -q /tmp/webrtc-outgoing.zip -d /tmp/webrtc-outgoing
python3 webrtc-sys/test/outgoing_silence/run.py \
  --webrtc-dir /tmp/webrtc-outgoing/mac-arm64-release
```

The runner preserves ABI defines from `webrtc.ninja`, including `NDEBUG`.
Equivalence checks are explicit runtime checks, so release builds retain them.
It exits nonzero on any failed check and emits CSV on stdout. The standalone
build recipe currently supports macOS only; it is not wired into SDK CI.

## Cases and checks

The deterministic harmonic fixture is generated in memory. It is a voiced-like
signal, not a speech-quality corpus. No recordings or external audio files are
needed. Each 101.4-second logical trace contains six 1.5-second active segments
and silence gaps of 0.2, 2, 30, 60 and 0.2 seconds. Controls at 12, 40 and 65
seconds update bitrate/loss; the last also resets the encoder.

Eight cases cover initial target bitrates of 24/48 kbps, mono/stereo and DTX
off/on. Complexity and low-rate complexity are 9; FEC is enabled. The same
ordered controls apply to all modes, including bitrate changes during silence.

- Continuous: encode every 10-ms frame.
- Deferred oracle: retain every encoder call, but process pending zeros at
  reference packet deadlines, new active input, controls or trace completion.
  With DTX off, deliberately retain the continuous schedule.
- Frozen counterexample: omit zero-input calls while advancing absolute input
  RTP timestamps. Check that active-region packets differ from the reference.

The deferred comparison includes every nonempty output's payload bytes, encoded
RTP timestamp and logical emission frame. The frozen comparison counts missing
or unequal packets whose encoded timestamp begins in an active region; it is
not a perceptual score and does not describe packets crossing a transition.

## Observed result

`results.csv` records a macOS arm64 release run against `webrtc-a545de4`. All
eight cases passed packet equivalence. With DTX on, each case used 497 logical
idle work events instead of 9,240 (94.6% fewer), deferring at most 40 frames
(400 ms). With DTX off, all 10,140 work events remained. Frozen-state active
packets differed in every case despite accounting for silence in input timestamps.

These counts are **not measured OS wakeups, CPU savings or session capacity**.
The oracle knows future reference outputs; all zero-frame encoder calls still
happen. This establishes a scheduling opportunity for the configurations tested,
not a production algorithm or an independent proof of deadline predictability.
The maximum observed batch is not a safe hard-coded sleep duration.

The harness does not exercise wall-clock adaptation, actual RTP/RTCP, receiver
NetEq, transport loss, callbacks, filters, E2EE, multiple sinks or OS scheduling.
Those remain integration requirements before changing runtime behavior.
