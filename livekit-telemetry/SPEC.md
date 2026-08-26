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
