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
attributes:                             # counts since the previous report
  lk.telemetry.uploads.sent: int        # batches accepted
  lk.telemetry.uploads.bytes: int       # compressed bytes accepted — what telemetry cost the uplink
  lk.telemetry.uploads.failed: int      # attempts that failed transiently (no answer, 429, 5xx)
  lk.telemetry.cache.batches: int       # batches waiting in the cache right now (a gauge)
  # the rest only when non-zero:
  lk.telemetry.uploads.timeouts: int    # attempts that hit export_timeout_ms
  lk.telemetry.uploads.unauthorized: int # 401/403 answers: tokens the collector refused
  lk.telemetry.holds.capped: int        # soft holds that reached the 60 s cap
  lk.telemetry.cache.write_errors: int  # batches the disk refused (full, gone), kept in memory
  lk.telemetry.dropped.queue_full: int  # records evicted from the in-memory queue
  lk.telemetry.dropped.cache_error: int # records no cache, not even memory, could take
  lk.telemetry.dropped.cache_full: int  # records evicted by the cache's size / file-count bound
  lk.telemetry.dropped.expired: int     # records in batches past the 24 h age limit
  lk.telemetry.dropped.corrupt: int     # records in cached batches that failed their CRC
  lk.telemetry.dropped.invalid: int     # custom events / attributes over the limits (rejected)
  lk.telemetry.dropped.rejected: int    # records the collector rejected (final 4xx/5xx, partial success)
  lk.telemetry.dropped.oversized: int   # single records larger than the collector accepts (413)
  lk.telemetry.dropped.throttled: int   # records evicted from the cache during a server-directed pause
  lk.telemetry.dropped.rate_limited: int # discrete events dropped by the flood guard
cadence: appended to the next upload whenever a loss, a failure, a refusal, a capped hold or a
         disk error happened since the previous report — never its own request, never persisted
         on its own, so a broken uploader never reports through itself — and once at shutdown as
         the session summary, so fleet-wide success rates have denominators. Losses by policy
         (a project that receives nothing, the opt-out) are local only (`Telemetry::stats`).
platforms: all
```

```yaml
event: lk.device.thermal.changed
area: device
attributes:
  lk.device.thermal.state: enum(nominal | fair | serious | critical)
cadence: on change (+ initial value on the first `set_device_state`); `unknown` (no thermal
         source on the platform, the default) is no reading: no event, no stretch
platforms: ios, macos, android — optional elsewhere
```

```yaml
event: lk.device.low_power.changed
area: device
attributes:
  lk.device.low_power.enabled: bool
cadence: on change (+ initial value); `None` (no source on the platform, the default) is no
         reading: no event, no stretch
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
  network.connection.type: enum(wifi | cell | wired | vpn | bluetooth | other | unavailable | unknown)   # OTel semconv
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
  lk.device.audio_route.reason: enum(new_device | old_device_unavailable | category_change | override | wake_from_sleep | no_suitable_route | route_configuration_change | unknown)   # AVAudioSession names; `unknown` where the platform gives none
  lk.device.audio_route.outputs: string  # comma-separated enum(speaker | receiver | wired_headset | bluetooth | car_audio | air_play | hdmi | usb | other)
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

```yaml
event: lk.device.capture.failed
area: device
severity: warn
attributes:
  lk.device.capture.device: enum(camera | microphone | screen_share)
  lk.device.capture.reason: enum(permission_denied | not_found | in_use | disconnected | other)   # the getUserMedia failure taxonomy
cadence: on failure
platforms: all — ios: authorization status, capture interruptions; android: permission checks, camera callbacks; web: DOMException names
```

## Log records

A `TelemetryEvent` with an empty `name` is a plain log record (OTLP log without `event_name`):
`severity` + `body` (the message) + `code.function.name`, `code.file.path`, `code.line.number`
(semconv), `lk.log.source` (`sdk` | `ffi` | `webrtc`) and `lk.log.logger` (type, module or file). The
platform hands the core a typed `LogRecord` via `log(record)`; the core applies the floor: WebRTC only
at `error`, the SDK and the core at the configured `log_severity`, the core's own telemetry module
never. Only `warn` and `error` records
leave the device; `trace`/`debug`/`info` are dropped in `emit`.

The core's own warnings and errors (Rust `log` records from `livekit*` targets, never
`livekit_telemetry*`) reach the pipeline by themselves through `livekit-uniffi`'s log forwarder:
it copies them as `ffi` records — every JWT-shaped substring (`eyJ…` `.` … `.` …) masked as
`<jwt>`, so a token quoted in an error never leaves the device — and forwards the console entry
unchanged. The copy is active wherever the platform calls `log_forward_bootstrap` (Swift's
default `OSLogger` with `ffi: true` does), whatever level it passes: that level filters only what
is forwarded to the platform's console (the global `log` level is kept at `warn` or looser) — Rust allows one logger per process, and without the
forwarder the core's records go nowhere. Platforms must not feed forwarded Rust log entries
(what `log_forward_receive` returns) to `telemetry_log` / `log(record)`: the core already copied
them, so they would be counted twice. `telemetry_log` is for the platform's own and WebRTC's
lines.

## Spans

A span is **one attempt** at an operation. The scope (one Room connection lifetime, across
reconnects) is the trace; its id is generated by the core when the pipeline starts and rides on
every span and log record. Spans are exported when they end — never a long-lived scope span.

| Rule | Value |
|---|---|
| Names | `lk.connect`, `lk.reconnect`, `lk.publish`, `lk.subscribe` — verbs, never ids |
| Kind | `CLIENT` for connect/reconnect (a call to the SFU), `INTERNAL` otherwise |
| Status | OTel `Unset` on success **and** cancellation, `Error` (+ `error.type`, message) on failure |
| `lk.outcome` | always present: `ok` \| `error` \| `cancelled` — rollups read this, never the status |
| `error.type` | platform-defined, a type name (≤ 128 bytes), never a message: e.g. Swift sends `LiveKitError.<numeric code>`, `CancellationError` or the Swift error type; dashboards group by it per `service.name` |
| Checkpoints | span events in the span's envelope (`ws_open`, `join_recv`, `pc_connected`, `attempt 2 full`, …); real events stay log records pointing at the span via `span_id` |
| Limits | 128 events and 128 attributes per span (OTel defaults); 256 open spans per pipeline |

```yaml
span: lk.connect
kind: client
attributes:
  lk.connect.attempt: int          # 1 for the user-initiated connect
checkpoints:
  required: ws_open, signal, join_recv, pc_created          # every platform, in this order
  best-effort: engine, pc_connected, offer_sent, answer_sent, room_connected   # where the SDK has the moment
outcome: ok | error (error.type) | cancelled
```

Dashboards compute connect phases only from the required checkpoints; best-effort ones refine
a platform's own view and may be missing or ordered differently (Android reports seven of the
nine today).

```yaml
span: lk.reconnect
kind: client
attributes:
  lk.reconnect.reason: enum(signal_disconnected | publisher_failed | subscriber_failed | transport_failed | switch_candidate | network_changed | debug | unknown)
  lk.reconnect.mode: enum(quick | full)   # mode of the last attempt
  lk.reconnect.attempts: int
checkpoints: "attempt <n> <mode>" per attempt
outcome: ok | error | cancelled     # cancelled when disconnect() or a newer reconnect wins
```

```yaml
span: lk.publish
kind: internal
parent: the ambient span, when any; a pre-connect publish (a microphone published before the
        connect completes) is its own span in the session's trace — `lk.connect` has ended by then
attributes:
  lk.track.kind: enum(audio | video)
  lk.track.source: enum(camera | microphone | screen_share | screen_share_audio | unknown)
  lk.track.sid: string            # on success
outcome: ok | error (error.type) | cancelled
```

```yaml
span: lk.subscribe
kind: internal
starts: when the intent to subscribe exists — a remote publish under autoSubscribe, or the
        manual subscribe call. Tracks already in the room at join: platforms call
        `subscribe_started` for each at connect (the join response lists them), so `lk.subscribe`
        measures from join on every platform; a `subscribed` with no intent before it still opens
        the span (a fallback, measured from the confirmation)
ends:   at first media (the first inbound stats reading with bytes; the core sees it) → ok;
        unsubscribe / unpublish before media → cancelled;
        subscription failure → error; no media within 30 s → error (error.type = timed_out),
        enforced on the core's own clock (no reading needed) and kept through a later disconnect
owner:  the core (`Scope::subscribe_started / subscribed / track_ended / subscribe_failed`);
        an SDK only reports the remote track's lifecycle
attributes:
  lk.track.sid: string
  lk.track.kind: enum(audio | video)
  lk.track.source: enum(camera | microphone | screen_share | screen_share_audio | unknown)
  lk.participant.remote_identity: string
checkpoints: subscribed, first_media
```
