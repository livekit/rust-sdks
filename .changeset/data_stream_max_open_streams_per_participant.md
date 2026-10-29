---
livekit-data-stream: major
livekit: minor
livekit-ffi: minor
livekit-capture: patch
livekit-rpc: patch
---

Limit each remote participant to 500 concurrently open incoming data streams by default. Headers for new streams beyond the limit are dropped with an error log. The limit is configurable with `RoomDataStreamOptions::with_max_open_streams_per_participant` and `max_open_streams_per_participant` on the FFI `RoomDataStreamOptions`.

`incoming::Manager::new` now takes an `incoming::ManagerOptions` struct instead of a bare `max_payload_byte_length`. The `core-modules` `IncomingDataStreamManager` constructor and `polled_incoming_data_stream_manager` likewise take an `IncomingDataStreamManagerOptions` record (`max_payload_byte_length`, `max_open_streams_per_participant`, both optional) in place of the `max_payload_byte_length` argument.
