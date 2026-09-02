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

## Events

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
  lk.telemetry.uploads.failed: int      # failed upload attempts since the previous report
  lk.telemetry.uploads.sent: int        # batches accepted since the previous report
  lk.telemetry.cache.batches: int       # batches waiting in the cache right now
  lk.telemetry.dropped.queue_full: int  # events evicted from the in-memory queue (omitted when 0)
  lk.telemetry.dropped.cache_error: int # events lost because the cache could not store them
  lk.telemetry.dropped.rejected: int    # events the collector rejected (4xx)
  lk.telemetry.dropped.throttled: int   # events dropped inside a Retry-After window
cadence: appended to the next batch whenever a drop or upload failure happened since the
         previous report — never its own request, never persisted on its own (Sentry client
         report shape; reasons follow the OTel SDK self-metrics `error.type` values)
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

## Cadence policy

`flush_interval × factor`, capped at 4× (15 s → 60 s at the production cadence):

| Condition | factor |
|---|---|
| thermal `serious` | 2 |
| thermal `critical` | 4 |
| low-power mode | 2 |
| background | 2 |

## Log records

A `TelemetryEvent` with an empty `name` is a plain log record (OTLP log without `event_name`):
`severity` + `body` (the message) + attributes such as `code.function`, `code.file.path`,
`code.line.number`, `lk.log.type` (the SDK logger's category). Only `warn` and `error` records
leave the device; `trace`/`debug`/`info` are dropped in `emit`.

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
