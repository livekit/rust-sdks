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

The pipeline that batches, caches and uploads records is not part of the crate yet.

Event names, attributes and span rules are defined in [`SPEC.md`](SPEC.md). It is the contract
for the whole crate, including the parts not implemented yet.
