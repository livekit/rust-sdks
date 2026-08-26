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
