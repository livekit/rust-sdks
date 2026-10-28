# Requirements for deadline-driven outgoing silence

Status: proposed native integration contract. The adjacent experiment validates
an oracle-driven encoder comparison; this document does not describe an
implemented sender or establish that every required deadline can be predicted.

## Boundary and scope

The buffered `AudioTrackSource::InternalSource` currently calls its sinks every
10 ms, substituting synthetic zeros when it cannot deliver a full queued frame.
Avoiding some of that scheduling requires a contract across the source, sender
and encoder. An application wrapper that simply stops supplying frames does not
establish correct media timing or codec state. Correcting timestamps alone also
does not establish state equivalence, as the experiment demonstrates.

Only the source's own synthetic zero padding is eligible. Do not discard queued
PCM or infer interchangeable silence from VAD. DTX negotiation is not evidence
that the encoder has already entered a state where work may be deferred.

## Proposed contract

1. Maintain a monotonic 64-bit source sample timeline through known-silence spans.
   Project it to the negotiated RTP clock with correct wrap behavior. Capture
   completion, queue duration and RTCP sender-report mapping must retain their
   existing semantics while heavy processing is deferred.
2. Represent known silence as a span with sample boundaries and audio format.
   Capture arrival, clear and interruption remain ordered events. New PCM cannot
   retroactively replace an already elapsed silence span.
3. Let the encoder advertise an internal capability and the earliest action
   required for guaranteed-zero future input. Pending packet assembly, DTX
   entry/refresh and codec controls must be accounted for. Unsupported modes
   retain the ordinary 10-ms path. No SDK-side fixed drain timeout or inferred
   400-ms sleep is sufficient evidence of safety.
4. At a deadline, advance through every intervening zero frame in order using
   the normal encoder and preserve naturally produced payloads and media
   timestamps. This coalesces work scheduling; it does not eliminate the
   encoder's computational work. A fast-forward codec path is separate work
   requiring an additional state/quality argument.
5. New PCM and controls preempt the wait. Process earlier silence before applying
   bitrate/loss/reset/configuration changes or new input at its logical time,
   then recompute the deadline. Preserve normal packet boundaries and first
   active-frame latency.
6. Keep RTCP and congestion-control timers active independently. Preserve SSRC,
   RTP sequence/timestamp behavior and receiver expectations. Stream stop/start,
   fabricated packets and timestamp-only correction do not meet this contract.
7. Preserve empty-output callbacks, stats, audio levels, transformers, filters
   and E2EE ordering, or retain the current path for those configurations. A
   source with multiple sinks needs independent cursors and must honor the
   earliest sink deadline. An unsupported sink prevents source-level deferral.

Illustrative internal concepts are `KnownSilenceSpan { begin, end, format }`
and a codec-owned `NextRequiredAction`. Their exact API is unresolved; prefer
internal capabilities over a public timer knob. Any native interface change
requires rebuilding and pinning matching WebRTC artifacts.

## Ownership and acceptance gates

The source owns queued PCM and its timeline, the sender owns the deadline task,
and the encoder owns codec state. Callbacks must use weak/cancellable back-links.
Cancel and join deadline work before releasing the source or encoder. Existing
queues must remain bounded and shutdown must be observable.

Before a production change is considered safe, deterministic native tests must
compare against continuous sending and cover:

- Payloads, packet timing, RTP sequence/timestamp/SSRC, wrap and RTCP clock mapping.
- First/last active content and transitions within partially assembled packets.
- DTX entry/exit/refresh, DTX disabled, FEC/RED, format and packetization changes.
- Controls immediately before deadlines and active input; adaptation behavior.
- Clear/cancel/backpressure, capture completion and delayed PCM arrival.
- Creation before publication, no sinks, fanout, sink replacement and reconnect.
- Callback/stats/filter/E2EE contracts or verified conservative fallbacks.
- Receiver NetEq delay, concealment and onset under long pauses, loss and jitter.
- Repeated initialize/use/disconnect/shutdown, partial failure and cancellation,
  checking destruction and resource counts rather than merely invoking Drop.

Only after these pass should multi-session CPU, latency and capacity be measured
under load. The current experiment supplies no capacity estimate. If a reliable
codec-owned deadline cannot be implemented, retain continuous state advancement
and pursue independently validated encoder or queue optimizations.
