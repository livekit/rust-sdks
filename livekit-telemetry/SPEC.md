# Client telemetry spec

Source of truth for event names, attributes and cadences emitted by LiveKit client SDKs.
Additive-only by convention; LiveKit-defined names carry the `lk.` prefix, everything else
follows [OpenTelemetry semantic conventions](https://github.com/open-telemetry/semantic-conventions).

## Resource attributes

Set once per pipeline (`TelemetryConfig.resource`):

| Key | Who sets it | Example |
|---|---|---|
| `service.name` | platform SDK | `livekit-client-swift` |
| `service.version` | platform SDK | `2.9.0` |
| `os.name`, `os.version` | platform SDK | `iOS`, `18.5` |
| `device.model.identifier` | platform SDK | `iPhone16,1` |
| `telemetry.sdk.name/language/version` | core | `livekit-telemetry`, `rust`, `0.1.0` |

## Pipeline, sessions and destination

One pipeline per process — started at SDK init, so audio pre-initialization, permission failures
and connect attempts that never reach a server are captured — and one **session** per room (one
call). A session is a trace id plus the attributes attached to its records (`lk.room.sid`,
`lk.participant.identity`, …); spans, RTC windows and events are filed under the session that
produced them, and `session.id` (OTel semconv) is written on every record as an attribute. A log
record emitted inside a room's span is filed under that room's session; anything emitted outside
a session — device state, pre-room errors, self-telemetry — belongs to the pipeline's own process
session. Sessions are not ended: a room's last record is simply its last.

The pipeline may start **without a destination** (`endpoint: None`): it buffers and caches, and
uploads nothing until `set_server(url, token)` (Cloud: `https://<host>/observability/logs/otlp/v0`,
`Authorization: Bearer <token>`) or `set_destination(endpoint, headers)` — at the first connect, when the
server URL yields the endpoint (`https://<host>/observability/logs/otlp/v0`) and the token the
`Authorization` header. Calling it again (new token, new server) replaces the destination for
the batches that follow. Waiting for a destination is not an upload hold: it is uncapped, bounded
only by the cache.

## Events

An event with no `body` is exported with its name as the body as well as in `event_name`: log
viewers key their line on the body, and not every backend surfaces `event_name` yet.

```yaml
event: lk.ping
area: sdk
severity: info
attributes:
  lk.ping.seq: int        # optional, monotonically increasing per pipeline
cadence: on demand — pipeline smoke test, never emitted in production paths
platforms: all
```

```yaml
event: lk.telemetry.report
area: sdk (self-telemetry)
severity: info
attributes:
  lk.telemetry.uploads.sent: int        # batches accepted since the previous report
  lk.telemetry.uploads.bytes: int       # compressed bytes accepted — what telemetry cost the uplink
  lk.telemetry.uploads.failed: int      # attempts that failed transiently (network error, 5xx)
  lk.telemetry.uploads.timeouts: int    # attempts that hit export_timeout_ms (omitted when 0)
  lk.telemetry.cache.batches: int       # batches waiting in the cache right now
  lk.telemetry.holds.capped: int        # upload holds that reached the 60 s cap (data ≥ 1 min late)
  lk.telemetry.dropped.queue_full: int  # events evicted from the in-memory queue (omitted when 0)
  lk.telemetry.dropped.cache_error: int # events lost because the cache could not store them (disk full)
  lk.telemetry.dropped.cache_full: int  # events evicted from the cache by max_cache_bytes / max age
  lk.telemetry.dropped.rejected: int    # events the collector rejected (4xx)
  lk.telemetry.dropped.throttled: int   # events dropped inside a Retry-After window
  lk.telemetry.dropped.rate_limited: int # discrete events dropped by the flood guard
cadence: appended to the next batch whenever a drop, an upload failure or a capped hold happened
         since the previous report — never its own request, never persisted on its own (Sentry
         client report shape; reasons follow the OTel SDK self-metrics `error.type` values) —
         and once at shutdown as the session summary, so fleet-wide success rates have
         denominators. Never emitted after the collector disabled telemetry.
platforms: all
```

```yaml
event: lk.device.thermal.changed
area: device
attributes:
  lk.device.thermal.state: enum(nominal | fair | serious | critical)
cadence: on change (+ initial value on the first `set_device_state`)
platforms: ios, macos, android — optional elsewhere
```

```yaml
event: lk.device.low_power.changed
area: device
attributes:
  lk.device.low_power.enabled: bool
cadence: on change (+ initial value)
platforms: ios, macos, android — optional elsewhere
```

```yaml
event: lk.device.app_state.changed
area: device
attributes:
  lk.device.app_state: enum(foreground | background)
cadence: on change (+ initial value); entering background also forces a flush
platforms: all
```

```yaml
event: lk.device.memory.changed
area: device
attributes:
  lk.device.memory.pressure: enum(normal | warning | critical)
    # Apple: DispatchSource memory-pressure levels; Android onTrimMemory: RUNNING_LOW /
    # BACKGROUND → warning, RUNNING_CRITICAL / COMPLETE → critical
cadence: on change (+ initial value)
platforms: ios, macos, android — optional elsewhere
```

```yaml
event: lk.device.network.changed
area: device
attributes:
  network.connection.type: enum(wifi | cell | wired | unavailable | unknown)   # OTel semconv
  lk.device.network.expensive: bool     # cellular / hotspot (NWPath.isExpensive, metered)
  lk.device.network.constrained: bool   # Low Data Mode / Data Saver / navigator.connection.saveData
cadence: on change of any attribute (+ initial value)
platforms: ios, macos, android — web: Chromium only
```

```yaml
event: lk.device.battery.changed
area: device
attributes:
  hw.battery.charge: double                        # 0.0–1.0 (OTel hardware semconv)
  hw.battery.state: enum(charging | discharging)   # OTel hardware semconv
cadence: on charging change and when the level crosses 20 % or 10 % unplugged — never per
         percent; silent where the level is unknown (desktops, tvOS)
platforms: ios, android — optional elsewhere
```

```yaml
event: lk.device.audio_route.changed
area: device
attributes:
  lk.device.audio_route.reason: string   # AVAudioSession route-change reason name
  lk.device.audio_route.outputs: string  # comma-separated output port types (speaker, bluetooth_a2dp, …)
cadence: on change
platforms: ios — android: audio device callbacks; optional elsewhere
```

```yaml
event: lk.device.audio.interruption
area: device
attributes:
  lk.device.audio.interruption: enum(began | ended)
cadence: on change
platforms: ios — android: audio focus loss/gain; optional elsewhere
```

## Cadence policy

`flush_interval × factor` and `stats_window × factor`, capped at 4× (15 s → 60 s at the
production cadence). Factors multiply; a change applies at the next tick, and a *shorter* period
applies at once (pressure relieved → no waiting out a stretched period).

| Condition | factor | source |
|---|---|---|
| thermal `serious` | 2 | host, `DeviceState.thermal` |
| thermal `critical` | 4 | host |
| memory pressure `warning` | 2 | host, `DeviceState.memory` |
| memory pressure `critical` | 4 | host |
| low-power mode | 2 | host |
| background | 2 | host |
| battery ≤ 20 % and unplugged | 2 | host, `DeviceState.battery_*` |
| constrained network (Low Data Mode / Data Saver) | 2 | host, `DeviceState.network_constrained` |
| encoder CPU-limited: an outbound track's `qualityLimitationDurations.cpu` grew within the last 60 s | 2 | core, from `record_stats` |

CPU is never measured by the pipeline itself (measuring CPU costs CPU): thermal state is the OS's
judgement and `qualityLimitationReason` is WebRTC's. `getStats()` polling stays on the SDK's
existing ~1 s timer; the *window* stretches, not the reading.

## Upload policy — telemetry never wins over media

Uploads are shaped, not just batched:

- **One request in flight**, oldest batch first; a failure pauses the cache for 60 s (throttling:
  see `lk.telemetry.report`).
- **Budget:** at most `max_batches_per_upload` (default 4) cached batches per tick while a session
  may be live, so a backlog (offline period, previous launch) replays at ~4 × 20 KB per 15 s
  ≈ 40 kbps next to a call. `shutdown` drains without the budget.
- **Holds** — nothing is sent, everything keeps flowing into the write-ahead cache — while:
  - an `lk.connect` or `lk.reconnect` span is open (signaling and ICE/DTLS own the uplink),
  - the device asks for quiet: constrained network, or battery ≤ 10 % unplugged (the Datadog
    rule). Device holds survive `shutdown`; the connect hold ends with the call.
  A hold lasts at most 60 s, then one batch goes out and the hold starts over — the hard cap
  that bounds the policy when its signals lie. `qualityLimitationDurations.bandwidth` is
  deliberately *not* a hold: WebRTC reports it for minutes during a normal ramp-up and for as
  long as an encoder stalls (an iPhone camera at 0 kbps held uploads for 8 minutes, one batch
  per minute through the cap). Yielding to media on the wire is the transport's job
  (`Priority: u=7`, Apple's background service class).
- **Bytes:** bodies are gzipped (level 1, `Content-Encoding: gzip`) when cached, so a batch is
  5–10× smaller on disk and on the wire and a replay costs no CPU. A request never carries more
  than `max_batch_bytes` (1 MiB, estimated before compression) or `max_batch_size` (512) records;
  when the queue reaches `flush_threshold_bytes` (256 KiB) it is exported at once instead of at
  the next tick — "every 15 s or at 256 KB".
- **Priority hints:** every request carries `Priority: u=7` (RFC 9218, lowest urgency) for
  HTTP/2+ hops that implement it, and the host transport marks the local traffic class as
  background — Apple `URLSessionConfiguration.networkServiceType = .background`
  (`NET_SERVICE_TYPE_BK`, below best effort in the local stack and Wi-Fi AC_BK), browsers
  `fetch(…, { priority: "low" })`, Android has no per-request class (WorkManager for deferred
  work; `socket.trafficClass` is best-effort).
- **Threads:** pipeline work runs on the SDK's runtime; none of it is on a media or UI thread and
  `emit`/`record_stats` never block on it.

## Log records

A `TelemetryEvent` with an empty `name` is a plain log record (OTLP log without `event_name`):
`severity` + `body` (the message) + `code.function.name`, `code.file.path`, `code.line.number`
(semconv), `lk.log.source` (`sdk` | `core` | `webrtc`) and `lk.log.logger` (type, module or file). The
platform hands the core a typed `LogRecord` via `log(record)`; the core applies the floor: WebRTC only
at `error`, the SDK and the core at the configured `log_severity`, the core's own telemetry module
never. Only `warn` and `error` records
leave the device; `trace`/`debug`/`info` are dropped in `emit`.

## Custom events

Consumers — apps, or an SDK's platform-specific extras — emit their own events with
`emit_custom(name, attributes)` (Swift: `room.emitTelemetryEvent(_:attributes:)`). The core
prefixes the name with `custom.` (`acme.checkout` → `custom.acme.checkout`), so a custom event
can never collide with, or spoof, an `lk.*` event and the backend can filter or quota the
namespace as a whole. Attributes keep the caller's namespace. Severity is `info`; custom events
count against the flood guard like any discrete event.

## Flood guard

Discrete events (`emit`) are capped at `max_events_per_10min` (default 300, design doc); what
exceeds it is dropped and reported as `lk.telemetry.dropped.rate_limited`. `lk.rtc.stats.sample`
windows and `lk.telemetry.report` are exempt.

```yaml
event: lk.rtc.stats.sample
area: rtc
severity: info
cadence: one per track and direction per stats window (default 15 s, stretched by the cadence
         factor); closed early on background and shutdown. Produced by the core from raw
         `record_stats` readings (1–2 s getStats polling on the platform).
attributes:
  lk.track.sid: string
  lk.track.kind: enum(audio | video)
  lk.track.direction: enum(inbound | outbound)
  lk.rtc.codec: string                      # mimeType, when known
  lk.rtc.window_ms: int                     # actual window length
  lk.rtc.samples: int                       # readings in the window
  # cumulative counters — the last reading's value, monotonic (W3C webrtc-stats model)
  lk.rtc.bytes: int
  lk.rtc.packets: int
  lk.rtc.packets_lost: int                  # inbound
  lk.rtc.freeze_count: int                  # inbound video
  lk.rtc.freezes_duration_ms: int           # inbound video
  lk.rtc.concealed_samples: int             # inbound audio
  lk.rtc.concealment_events: int            # inbound audio
  lk.rtc.jitter_buffer_delay_ms: int        # inbound
  lk.rtc.jitter_buffer_emitted_count: int   # inbound
  lk.rtc.quality_limitation.bandwidth_ms: int   # outbound video
  lk.rtc.quality_limitation.cpu_ms: int         # outbound video
  # gauges — min / max / avg over the window
  lk.rtc.jitter_ms.{min,max,avg}: double
  lk.rtc.rtt_ms.{min,max,avg}: double       # remote-inbound RTT for outbound, candidate-pair for inbound
  lk.rtc.fps.{min,max,avg}: double          # video
  lk.rtc.audio_level.{min,max,avg}: double  # audio
platforms: all
```

## Spans

A span is **one attempt** at an operation. The session (one Room connection lifetime, across
reconnects) is the trace; its id is generated by the core when the pipeline starts and rides on
every span and log record. Spans are exported when they end — never a long-lived session span.

| Rule | Value |
|---|---|
| Names | `lk.connect`, `lk.reconnect`, `lk.publish`, `lk.subscribe` — verbs, never ids |
| Kind | `CLIENT` for connect/reconnect (a call to the SFU), `INTERNAL` otherwise |
| Status | OTel `Unset` on success **and** cancellation, `Error` (+ `error.type`, message) on failure |
| `lk.outcome` | always present: `ok` \| `error` \| `cancelled` — rollups read this, never the status |
| Checkpoints | span events in the span's envelope (`ws_open`, `join_recv`, `pc_connected`, `attempt 2 full`, …); real events stay log records pointing at the span via `span_id` |
| Limits | 128 events and 128 attributes per span (OTel defaults); 256 open spans per pipeline |

```yaml
span: lk.connect
kind: client
attributes:
  lk.connect.attempt: int          # 1 for the user-initiated connect
checkpoints: ws_open, signal, join_recv, pc_created, engine, pc_connected, offer_sent, answer_sent, room_connected
outcome: ok | error (error.type = LiveKitError.<case> | CancellationError | <Swift type>) | cancelled
```

```yaml
span: lk.reconnect
kind: client
attributes:
  lk.reconnect.reason: string      # what triggered the cycle
  lk.reconnect.mode: enum(quick | full)   # mode of the last attempt
  lk.reconnect.attempts: int
checkpoints: "attempt <n> <mode>" per attempt
outcome: ok | error | cancelled     # cancelled when disconnect() or a newer reconnect wins
```

```yaml
span: lk.publish
kind: internal
parent: the ambient span, when any (the connect span for a pre-connect microphone)
attributes:
  lk.track.kind: enum(audio | video)
  lk.track.source: enum(camera | microphone | screenShare | screenShareAudio | unknown)
  lk.track.sid: string            # on success
outcome: ok | error (error.type) | cancelled
```

```yaml
span: lk.subscribe
kind: internal
starts: when the intent to subscribe exists — a remote publish under autoSubscribe, or the
        manual subscribe call
ends:   at first media (the first stats reading with bytes received; 1 s granularity) → ok;
        unsubscribe / unpublish before media → cancelled;
        subscription failure → error; no media within 30 s → error (error.type = LiveKitError.timedOut)
attributes:
  lk.track.sid: string
  lk.track.kind: enum(audio | video)
  lk.track.source: string
  lk.participant.remote_identity: string
checkpoints: subscribed, first_media
```
