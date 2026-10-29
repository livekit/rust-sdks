---
livekit-data-stream: major
livekit: minor
livekit-ffi: minor
---

Limit each remote participant to 500 concurrently open incoming data streams by default. Headers for new streams beyond the limit are dropped with an error log. The limit is configurable with `RoomDataStreamOptions::with_max_open_streams_per_participant`, `max_open_streams_per_participant` on the FFI `RoomDataStreamOptions`, and an optional `max_open_streams_per_participant` argument on the `core-modules` incoming data stream managers. `incoming::Manager::new` now takes this limit as a second argument.
