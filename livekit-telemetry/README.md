# LiveKit Telemetry

**Important**:
This is an internal crate for client telemetry in LiveKit client SDKs and is not meant to be
used directly.

It currently holds the shared data model and its encoding:

- **Records.** `TelemetryEvent`, `LogRecord`, `Attribute` / `AttributeValue`, `Severity` and
  `SpanOutcome`: the events, log lines and span outcomes a platform SDK hands over.
- **Buffers.** Bounded, drop-oldest queues for records and spans, each filed under the session
  (trace) it belongs to; every drop is counted.
- **Encoding.** [OTLP/HTTP](https://opentelemetry.io/docs/specs/otlp/) protobuf logs and traces,
  using the `opentelemetry-proto` message types only (no gRPC stack).
- **Transport and destinations.** `TelemetryTransport`, the seam that moves an encoded batch off
  the device (`NetTransport` over `livekit-net` with the `net` feature), and the routing that
  turns a Room's server URL and token into its LiveKit Cloud ingest URL and credential.

The pipeline that batches, caches and uploads records is not part of the crate yet.

Event names, attributes and span rules are defined in [`SPEC.md`](SPEC.md). It is the contract
for the whole crate, including the parts not implemented yet.

Collector responses are limited to 65,536 decoded body bytes. Overflow is retryable and preserves a valid Retry-After delay; no truncated body is used to classify acceptance or a project refusal. NetTransport uses livekit-net's opt-in bounded client view. Its native backend stops during the read, including gzip decoding; ordinary HTTP requests remain uncapped. A host TelemetryTransport must enforce the same limit in its reader. The core also checks already-buffered custom/FFI/pulled responses, but this cannot bound allocation that already occurred in the host.
