# Validation and limitations

The candidate preserves exercised audio behavior, but **has not demonstrated a
process CPU saving on macOS**. Do not deploy it as a proven capacity improvement.

## Native checks

Mac arm64 release validation used the SDK's `webrtc-a545de4` archive, rebuilding
all affected encoder objects and the sender against m150 sources. This is a
local validation overlay, not a clean full WebRTC release build. The Linux CI
workflow rebuilds the complete native tree before testing and producing FFI.

- 96 deterministic cases pass with a simulated clock: mono/stereo, 24/48 kbps,
  DTX off/on, 10/20/40/60/120-ms packets, queue backlog and RED at 20 ms.
- Completed payloads, encoded timestamps, logical emission frames, DTX-empty
  callbacks and RED metadata match continuous encoding. Loss/bitrate controls
  during silence and a reset are included. PCM is generated in memory.
- Control ordering, new-audio preemption, stale budget publication, unsupported
  capability caching, concurrent admission and 32 joined teardown cycles pass.
- The queue checks also pass AddressSanitizer/UndefinedBehaviorSanitizer.
- The built-in Opus, G722, PCMU and PCMA factories create working encoders.

`results.csv` records the deterministic scheduling counts. With ordinary 20-ms
Opus packets and a drained queue, the 101.4-second trace reduces roughly 10,143
encoder-queue submissions to 5,530. This is about half the idle submissions and
45% of total submissions in this mostly idle trace. Every encoder call remains.
These are task counts, not measured OS wakeups, CPU savings or session capacity.

## Actual RTC checks

Matching baseline/candidate Mac FFI builds were tested with generated tones and
buffered 24-kHz mono sources. Both explicitly used dual peer connections and the
same negotiated `minptime=10;usedtx=1;useinbandfec=1` Opus parameters.

All four known prefixes arrived in each run, including after 2-, 30- and
60-second pauses. Observed onset ranges were 104–128 ms baseline and 114–118 ms
candidate. Neither run accumulated non-silent concealed samples. RTP sequence
numbers stayed contiguous across intentional DTX gaps; maximum observed
wall-clock/RTP gap differences were 23.5 ms baseline and 27.4 ms candidate.
This is a small local integration sample, not a perceptual or network-impairment
certification.

## CPU results

Simultaneous baseline/candidate processes each ran four rooms, with incoming
generated audio active, DTX and RED enabled. Three 15-second measured repetitions
per mode followed warmup. Libraries used the same SDK revision and build profile;
the baseline used the same rebuilt codec objects without sender coalescing.
Negotiated codec formats matched before and after every included measurement.

| Mode | Paired candidate CPU changes | Median paired change |
| --- | --- | --- |
| Outgoing idle | -1.6%, +5.0%, +3.8% | +3.8% |
| Outgoing active | +1.1%, -0.6%, -2.5% | -0.6% |

Positive means more CPU. These small, mixed results do not establish a saving.
The conservative scheduler is a testable implementation, not an established
performance fix. Linux measurements and a complete native rebuild remain gates.
No session-capacity multiplier follows from these data.

An earlier single-PC comparison negotiated different DTX/stereo parameters
between variants. It is excluded. The benchmark now explicitly controls
connection mode and rejects mismatched or changing codec formats. Ring-buffer
and Opus-complexity experiments are outside this patch.
