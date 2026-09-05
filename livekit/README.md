# LiveKit Client SDK

The official client SDK for [LiveKit](https://livekit.com).

Use this SDK to add realtime video, audio and data features to your Rust app.

## FlexFEC

Published video can be protected with proactive FlexFEC-03 forward error
correction, letting receivers (the LiveKit SFU or peers) repair packet loss
without waiting for retransmissions:

```rust
use livekit::options::{FecProtection, TrackPublishOptions};

let mut options = RoomOptions::default();
options.fec_enabled = true;
let (room, events) = Room::connect(&url, &token, options).await?;

room.local_participant()
    .publish_track(track, TrackPublishOptions {
        fec: FecProtection::Medium,
        ..Default::default()
    })
    .await?;
```

Notes:

- `RoomOptions::fec_enabled` enables FEC negotiation for published tracks.
  Receivers negotiate FlexFEC automatically and need no room option.
- Each video track selects `Disabled`, `Low`, `Medium`, or `High` protection.
  These correspond to 0%, 15%, 25%, and 35% protection. The default is
  `Disabled`.
- Protection uses a fixed one-frame block and a random-loss mask.
- `Room::fec_sender_stats` reports the aggregate FEC send rate.
- FlexFEC protects the first simulcast layer only, publishing with
  `simulcast: false` is recommended for protected tracks.
- FEC spends bandwidth proactively: it pays off on links where loss is
  bursty or the RTT makes retransmissions slow, and costs throughput on
  clean or low-latency links.
