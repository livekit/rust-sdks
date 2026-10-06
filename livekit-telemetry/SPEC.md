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

## Pipeline, scopes and destination

One pipeline per process — started at SDK init, so audio pre-initialization, permission failures
and connect attempts that never reach a server are captured — and one **scope** per room (one
call). A scope is a trace id plus the attributes attached to its records (`lk.room.sid`,
`lk.participant.identity`, …); spans, RTC windows and events are filed under the scope that
produced them, and `session.id` (OTel semconv) is written on every record as an attribute. A log
record emitted inside a room's span is filed under that room's scope; anything emitted outside
a scope — device state, pre-room errors, self-telemetry — belongs to the pipeline's own process
scope. Scopes are not ended: a room's last record is simply its last.

### Destination and credentials

The platform passes exactly two things per room, through `Scope::set_server(url, token)`: the
LiveKit server URL the room connects to and the participant token it connects with — at connect,
and again with **every** refreshed token (the SFU sends one right after join, then every few
minutes). The call is cheap and idempotent. There is no client-side endpoint, header or sink.

- **Ingest URL:** derived from the server URL's host, `https://<host>/observability/client/{logs,traces}/otlp/v0`,
  for LiveKit Cloud hosts only (`*.livekit.cloud`, including `*.staging.livekit.cloud`). Any other
  host (self-hosted, OSS) has no ingest: one local warning, and that room's records are dropped
  at the door instead of cached.
- **Token:** `Authorization: Bearer <token>`. The core reads the token's *unverified* claims: the
  observability grant (`observability.write` or `observability.clientWrite`) and `exp`. It never
  sends a token known to be expired or refused; batches wait for the next token instead
  (a hard hold). A project whose first token has no grant never opted in: nothing is collected
  for it. A refreshed token without the grant (today's SFU drops it) does not replace a granted
  one that is still valid. Tokens live in memory only — never in a batch, never on disk.
- **Server URL validation:** parsed with WHATWG URL rules, never string-matched. A token is only
  sent to `https://<project>.livekit.cloud/…` built from the parsed host alone: TLS scheme
  (`wss`/`https`), a domain under the Cloud suffix with a label of its own, the default port, no
  userinfo.
- **Ownership:** every record captures its owner when it is captured — its session and the
  project that session is routed to at that moment — and keeps it: a Room that reconnects to
  another project takes nothing queued or cached along. Credentials are keyed by (project,
  session): a live Room uploads with its **own** latest token for that project, never another
  Room's. A Room's records captured before it had a server go to its own first project, never to
  another Room's. Process-level records (device state, pre-room errors, self-telemetry) go to
  the project most recently handed a token, with that project's latest token; so do batches from
  a previous launch, which wait — up to the 24 h age limit — for a token of the same project.
  The answer to a request is attributed to the project it was sent to — its 404, disable or
  pause never lands on another project. A session's credentials stay while the session is alive
  (its Room, or records, windows or spans still referencing it) — for every project it was routed
  to, so records captured for an earlier project can still be sent; that is the residual: a live
  Room keeps one credential per project it has used — and while cached batches need them;
  project-level copies only while a live session is routed there or the backlog has batches for
  it. A token the collector refused is recorded by identity (a hash) and never sent again from
  any slot, until it expires (a token without `exp` stays refused for the process; like the
  per-host project table, which keeps one small entry per host ever seen, that grows only with
  what a process meets — bounded in practice, not by a cap). A past,
  negative or non-numeric `exp` counts as expired.
- **Waiting** for a first destination or a usable token is uncapped, bounded only by the cache.

Local end-to-end tests point everything at an OpenTelemetry collector of their own with the
`LK_TELEMETRY_ENDPOINT` environment variable, read by the core at start (a base URL gets
`/v1/logs` and `/v1/traces`; a URL ending in `logs` is used as is). It is not part of any platform
API. With it, every batch goes there without Cloud rules or tokens.

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

## Cadence policy

`flush_interval × factor` and `stats_window × factor`, capped at 4× (60 s → 4 min at the default
cadence: one export and one RTC window per minute, conservative for the collector's shared
per-project quota). Factors multiply; a change applies at the next tick, and a *shorter* period
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
judgement and `qualityLimitationReason` is WebRTC's. The core also paces the platform's
`getStats()` polling (`Scope::stats_poll_interval_ms`): every second while a subscribe waits for
its first media, else twice per (stretched) window — 30 s by default.

## Upload policy — telemetry never wins over media

Uploads are shaped, not just batched:

- **One request in flight**, oldest batch first, polled by the exporter's loop: commands, ticks,
  subscribe deadlines and wake-ups are served while it is out. Each answer is classified by the
  core (see *Collector answers*); a transport returns status, headers and body and fails only
  without a response. A pause stops uploads **to that destination** — other projects carry on —
  never collection: records keep landing in the cache and ship when it lifts.
- **Retry:** local backoff starts at 1 s and doubles per consecutive failure up to 60 s, with
  full jitter (a uniform wait in `[0, backoff]`). A delay the server names — `Retry-After`
  (delay-seconds or HTTP-date, RFC 9110 §10.2.3) or `RetryInfo.retry_delay` — is honored in
  full even when longer, validated (garbage ignored, negative → now, overflow ignored) and
  clamped only to the 24 h age limit; neither shutdown nor a hold's escape cuts it short.
  Running out of patience never deletes: only the cache's age and size bound what is kept.
- **Budget:** while a Room is in a call (it has a server and has not disconnected), at most
  `max_batches_per_upload` (default 4) requests per flush interval — an allowance refilled at
  each tick, 413 halves included, each request at most `max_batch_bytes` of protobuf before gzip
  — however many wake-ups happen; a backlog (offline period, previous launch) waits its turn
  oldest first. With no Room in a call nothing is metered: each pass sends the whole cache. A new
  batch is encoded only at the tick, when the queue crosses `flush_threshold_bytes`, or when the
  app enters the background; a lifted route, token or hold only re-runs a pass over what is
  cached (within the allowance); a subscribe or a cadence change only re-reads deadlines.
  Entering the background and an explicit `flush()` drain the whole cache without the allowance,
  within every hold and pause. `shutdown` drains without the budget within
  `export_timeout_ms`, then cancels the request on the wire and stops the exporter; what did not
  go out stays cached.
- **Holds** — nothing is sent, everything keeps flowing into the write-ahead cache. The policy is
  evaluated before every request, not once per pass, and a hold changing (or the app entering the
  background, or the cadence changing) wakes the exporter:
  - *hard* (no escape, for as long as they last): the device is offline; no usable token
    (missing, expired, grant-less, refused); the project disabled data recording; the opt-out.
  - *soft* (at most 60 s, on its own clock rather than the next tick, then one batch goes out and
    the hold starts over — the cap that bounds the policy when its signals lie): an `lk.connect` or `lk.reconnect` span is open (signaling
    and ICE/DTLS own the uplink); Low Data Mode / Data Saver; battery ≤ 10 % unplugged. The cap is
    not scheduled while a hard hold is on; it resumes when the hard hold lifts.
  `qualityLimitationDurations.bandwidth` is deliberately *not* a hold: WebRTC reports it for
  minutes during a normal ramp-up and for as long as an encoder stalls.
- **Bytes:** bodies are gzipped (level 1, `Content-Encoding: gzip`) when cached, so a batch is
  5–10× smaller on disk and on the wire and a replay costs no CPU; a cached batch is CRC-checked
  before it is sent. A request never carries more than `max_batch_size` (512) records, nor more
  than `max_batch_bytes` (1 MiB) of encoded protobuf — checked on the encoded body, session
  attributes and the self-report included (the report takes one of the `max_batch_size`
  places); a batch over it is halved until it fits, and a single record over it is dropped and
  counted as `oversized`. Caller strings a span or scope retains are bounded: names, keys and
  error types ≤ 128 bytes, values and Room identities ≤ 1 KiB (anything longer is not kept and
  is counted as `invalid`). When the queue reaches
  `flush_threshold_bytes` (256 KiB) it is exported at once instead of at the next tick.
- **Backlog:** nested bounds, every eviction counted — the queue (`max_queue_size`, 2048
  records), the cache's size (`max_cache_bytes`, 4 MiB compressed) and file count (512
  batches), and age (24 h, enforced at start and while running; the monotonic clock must agree
  for this launch's own batches, so a clock set forward cannot expire them). Oldest goes first.
- **Durability:** a batch is committed when the cache's `push` returns: `FileCache` writes a
  `.tmp`, `fsync`s it, renames it into place and `fsync`s the directory (on Unix; elsewhere a
  directory cannot be synced and that step is skipped). If any step fails the file is removed
  and the batch is kept in memory instead (counted as a write error) — never claimed committed.
  A 413 split is journaled (halves written, synced and renamed to `<parent>@<half>.pend`,
  directory synced, parent deleted = commit point, halves published); an interrupted split is
  finished or rolled back when the cache opens, before anything is evicted, so every record is
  there exactly once. A split that fails after its commit point has succeeded: its halves wait in
  the journal and the next listing of the cache publishes them (so they upload, and an opt-out
  purges and counts them). Splits and recovery share a process-wide lock, so a reconfigure's
  cache on the same directory never sees the draining pipeline's split half-done; stray files
  are swept only when a cache opens. A crash loses what was not committed yet — records since the last tick
  (≤ one flush interval, ≤ 2048 queued), open spans, open RTC windows. Duplicates: a crash
  between an answer and its delete sends that one batch again at the next launch; a delete that
  fails without a crash leaves the batch pending deletion (never sent again in this launch, but
  sent once more after a restart if it is still there). A disk that refuses writes gets batches
  kept in memory (bounded by `max_cache_bytes`, oldest evicted and counted), which survive
  failures but not the process. A batch that exists but cannot be read right now (file
  protection, permissions) is kept, not counted corrupt.
- **Priority:** every request carries `Priority: u=7` (RFC 9218, lowest urgency) for HTTP/2+
  hops that implement it.
- **Redirects:** a 3xx the transport returns drops the batch. Transports must not forward
  `Authorization` across origins; the `livekit-net` native client strips it when a redirect
  changes the host or the port (tested). A scheme-only change on the same explicit port is not
  covered by those tests.
- **Threads:** pipeline work runs on the SDK's runtime; none of it is on a media or UI thread and
  `emit`/`record_stats` never block on it.

### Collector answers

| Answer | Batch | Pipeline |
|---|---|---|
| 2xx | removed | — |
| 2xx with OTLP `partial_success` | removed; rejected records counted, never retried | — |
| 400, 3xx, other 4xx | dropped, counted `rejected` | — |
| 413 | replaced by two halves in one cache transaction (both committed where the batch was, on disk stays on disk, nothing evicted in between) and retried at once, down to one record; if the cache cannot take the halves the batch stays whole; a lone oversized record is dropped, counted `oversized` | — |
| 401/403 "data recording is disabled by owner" | purged with the project's whole cache | project silent for the process |
| other 401/403 | kept | that token is never sent again; the project's batches wait for the next token |
| 404 | purged with the project's cache | project silent until its next token (a new connect or refresh) |
| 429 | kept | paused for `Retry-After`, else `RetryInfo`, else 60 s |
| 503 with `Retry-After`/`RetryInfo` | kept | paused for that long |
| 502, 503, 504; any error with `RetryInfo` (Cloud's retryable 500) | kept | paused for the named delay, else backoff |
| other 5xx | dropped, counted `rejected` | — |
| no answer: timeout, connection, DNS, TLS | kept | backoff; never a capability decision |
| invalid request (transport-side) | dropped, counted `rejected` | — |

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

## App API: custom events, correlation attributes, opt-out

Three things an app can do; everything else is the SDK's.

- **Custom events** are room-scoped: `Scope::emit_custom(name, attributes)` (Swift:
  `room.emitTelemetryEvent(_:attributes:)`). The core prefixes the name with `custom.`
  (`checkout` → `custom.checkout`), so a custom event can never collide with, or spoof, an
  `lk.*` event and the backend can filter or quota the namespace as a whole. Severity is `info`;
  custom events count against the flood guard like any discrete event.
- **Correlation attributes** are room-scoped too: `Scope::set_attribute(key, value | nil)` —
  `app.call_id`, `enduser.id` — inherited by the room's subsequent logs, spans, events and RTC
  windows. Values are snapshotted when a record is captured: a later change never rewrites what
  is queued, and a change closes the Room's open RTC windows first, so a window never mixes
  readings taken under two values. A custom event's own attributes override the room's. There is no public
  process-wide attribute (a mutable global would leak across simultaneous rooms); the
  process-level set is internal, for SDK metadata.
- **Limits:** names and keys ≤ 128 UTF-8 bytes, string values ≤ 1024 bytes, ≤ 64 attributes per
  room and per event. SDK-owned keys are reserved: `lk.*` (room, participant, track, outcome …)
  and `session.id`. Anything else is rejected — never truncated, a truncated id collides — and
  counted as `lk.telemetry.dropped.invalid`.
- **Opt-out** is process-wide and synchronous: when `telemetry_disable()` returns, no scope is
  handed out, every existing scope and instrument captures nothing and later configures are
  refused; the purge — cancel scheduled work, delete everything not yet sent (queue, open spans
  and windows, every cached batch on disk) — then runs on the core's runtime
  (`telemetry_shutdown` / `telemetry_flush` await it; `global::disable()` returns it as a
  future).
  Install, instrument start/stop and the opt-out run under one lifecycle lock, so an instrument
  never starts after the opt-out stopped it. Every pipeline generation still alive (one replaced
  but still draining included) is revoked — the flag is checked under the lock of every structure
  that commits data (queue, spans, RTC windows, the cache's write gate), which the purge also
  takes, so nothing lands after it — then its exporter's request on the wire is cancelled and
  the exporter awaited, and later configures in the process are refused. The pull queue never
  hands out a request its exporter gave up on, whether the host awaits `next()` or polls
  `try_next()` (Dart, from a timer: no Rust future ever holds a continuation of an isolate that
  may die; such hosts pass no instruments either, so Rust never calls into them). Data already sent cannot be recalled: at most the
  one batch per generation already on the wire, or already handed to a pull-queue host. If the
  storage refuses a delete the purge is reported incomplete (logged; `global::disable` and
  `Telemetry::purge` return `false`; the exported `telemetry_disable` returns nothing). Platforms
  mark the call TODO pending the token discussion.

## Flood guard

Discrete events (`emit`) are capped at `max_events_per_10min` (default 300, design doc); what
exceeds it is dropped and reported as `lk.telemetry.dropped.rate_limited`. `lk.rtc.stats.sample`
windows and `lk.telemetry.report` are exempt.

```yaml
event: lk.rtc.stats.sample
area: rtc
severity: info
cadence: one per track and direction per stats window (default 60 s, stretched by the cadence
         factor); closed early on background, when the track leaves (`track_ended`), at
         disconnect and at shutdown. Produced by the core from raw readings — one getStats()
         report per peer connection (`record_peer_stats`), polled when the core says
         (`stats_poll_interval_ms`). Keyed by session and track: two rooms receiving the same
         track keep two windows.
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
  lk.rtc.quality_limitation.other_ms: int       # outbound video
  lk.rtc.pause_count: int                   # inbound video
  lk.rtc.pauses_duration_ms: int            # inbound video
  lk.rtc.silent_concealed_samples: int      # inbound audio
  lk.rtc.interruption_count: int            # inbound audio
  lk.rtc.interruptions_duration_ms: int     # inbound audio
  # gauges — min / max / avg over the window
  lk.rtc.jitter_ms.{min,max,avg}: double
  lk.rtc.rtt_ms.{min,max,avg}: double       # remote-inbound RTT for outbound, candidate-pair for inbound
  lk.rtc.fps.{min,max,avg}: double          # video
  lk.rtc.audio_level.{min,max,avg}: double  # audio
platforms: all
```

```yaml
event: lk.room.disconnected
area: session
severity: info (client_initiated) | warn (anything else)
cadence: once, when the Room leaves connected for good — never on a reconnect
attributes:
  lk.disconnect.reason: enum(client_initiated | duplicate_identity | server_shutdown | participant_removed | room_deleted | state_mismatch | join_failure | migration | signal_close | room_closed | user_unavailable | user_rejected | sip_trunk_failure | connection_timeout | media_failure | agent_error | reconnect_failed | unknown)   # the protocol's DisconnectReason, plus the client giving up
platforms: all
```

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

## Typed surface

Everything the SDKs have in common enters the core typed; the core owns the keys, the bodies and
the policy. `Attribute { key, value }` survives only as the open bag: `emit_custom`,
`set_attribute`, and `Span::set_attribute` for app-defined spans.

| Platform calls | The core produces |
|---|---|
| `Scope::set_server(url, token)` — at connect and on every token refresh | the room's destination: `https://<host>/observability/client/{logs,traces}/otlp/v0` for Cloud hosts, the token's grant and expiry read, its batches routed to its project with its own token |
| `Scope::emit_custom(name, attributes)`, `Scope::set_attribute(key, value)` | `custom.<name>` events and correlation attributes, validated and snapshotted (see *App API*) |
| `telemetry_disable()` | the opt-out, in effect when it returns: capture stops; everything unsent is then deleted |
| `TelemetryConfig.sdk: TelemetryResource { sdk: Sdk, sdk_version, os_name, os_version, device_model }` | `service.name = livekit-client-<sdk>`, `service.version`, `os.*`, `device.model.identifier`, plus `telemetry.sdk.*` |
| `log(LogRecord { severity, source: LogSource, body, logger, function, file, line, timestamp_ns, span_id })` | a record with `code.function.name`, `code.file.path`, `code.line.number`, `lk.log.source`, `lk.log.logger`; the per-source floor (WebRTC at `error`, own module never) |
| `Scope::set_room(RoomIdentity { sid, name, participant_sid, participant_identity })` | `lk.room.*`, `lk.participant.*` on every record of the scope |
| `Scope::start(SpanName, parent) -> Span`; `Span::detached(name)` | an OTLP span (`lk.connect` / `lk.reconnect` are `client`, the rest `internal`); `Reconnect { reason }` sets `lk.reconnect.reason` |
| `Span::step(SpanStep)` | a span event named `ws_open` … `room_connected`, `subscribed`, `first_media`, `attempt N quick|full` (which also sets `lk.reconnect.attempts` / `.mode`) |
| `Span::set_track(SpanTrack { sid, kind, source, remote_identity })` | `lk.track.sid`, `lk.track.kind`, `lk.track.source`, `lk.participant.remote_identity` |
| `Span::end(outcome, error)` / `fail(error)` / `cancel()` | status, `error.type`, `lk.outcome`; ending twice is a no-op |
| `Span::describe()` | `lk.connect: ws_open +1.49s, signal +0.03s, total 1.83s, ok` — the console line, identical on every platform |
| `Span::context()` | `TraceContext { trace_id, span_id }` for log correlation; `None` when detached |
| `device_event(DeviceEvent::{AudioRouteChanged, AudioInterruption, CaptureFailed})` | `lk.device.audio_route.changed`, `lk.device.audio.interruption`, `lk.device.capture.failed` with display bodies; every value is a shared enum (`AudioOutput`, `CaptureDevice`, `CaptureFailure`) |
| `Scope::subscribe_started(SpanTrack)`, `subscribed(SpanTrack)`, `subscribe_failed(sid, error_type)` | the `lk.subscribe` span, ended by the core at the first inbound reading with bytes, or `timed_out` after 30 s; tracks already in the room at join: `subscribe_started` at connect (a lone `subscribed` still opens the span, as a fallback) |
| `Scope::track_ended(sid)` | a pending subscribe ends (`cancelled`, or `timed_out` past its deadline); the track's last RTC window ships; its per-track state is retired |
| `Scope::record_peer_stats(Vec<RtcStat>, tracks: {MediaStreamTrack id → sid}, ts)` | one peer connection's raw `getStats()` report → the core finds each track's RTP streams (`trackIdentifier`, or the `media-source` an `outbound-rtp` names), resolves codec / RTT, converts seconds to ms and records one sample per stream |
| `Scope::stats_poll_interval_ms()` | when to poll `getStats()` next: 1 s while a subscribe awaits first media, else half the stretched window |
| `Scope::record_stats_report(sid, kind, direction, Vec<RtcStat>, ts)` | the same mapping for one track's report (platforms that poll per track) |
| `DisconnectReason::from_proto(i32)`, `ReconnectReason::from_proto(i32)` | the protocol numbers → the shared enums |
| `Scope::log(LogRecord)` | a record filed under the session without an ambient span (Dart has no task-local outside a zone); same floor and filters as `Telemetry::log` |
| `Scope::disconnected(DisconnectReason)` | `lk.room.disconnected` with `lk.disconnect.reason` — info when the client hung up, warn otherwise; pending subscribes end, the session's RTC windows ship and its state is retired |
| `RtcStatsSample.layer` (rid, ssrc or stats id) | simulcast layers folded into one monotonic series per track before windowing |

Timing rule: span calls are synchronous and stamp the clock inside the core, so the only skew is
the FFI call. Anything that may cross an executor hop before reaching the core (a log record) carries
its own `timestamp_ns` from capture. Context propagation — the "current" span — stays with the
platform runtime (task-local, coroutine context, zone); that is the one piece a core cannot own.
