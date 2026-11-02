# LiveKit Client SDK

The official client SDK for [LiveKit](https://livekit.com).

Use this SDK to add realtime video, audio and data features to your Rust app.

## FlexFEC

Published video can be protected with proactive FlexFEC-03 forward error
correction, letting receivers (the LiveKit SFU or peers) repair packet loss
without waiting for retransmissions:

```rust,ignore
use livekit::options::{FecProtection, TrackPublishOptions};

let (room, events) = Room::connect(&url, &token, RoomOptions::default()).await?;

room.local_participant()
    .publish_track(track, TrackPublishOptions {
        fec: FecProtection::Medium,
        ..Default::default()
    })
    .await?;
```

Notes:

- FlexFEC is negotiated for every published video track. Receivers negotiate it
  automatically and need no room option.
- Each video track selects `Disabled`, `Low`, `Medium`, or `High` protection.
  These correspond to 0%, 15%, 25%, and 35% protection. The default is
  `Disabled`, which sends no repair packets.
- Protection uses a fixed one-frame block and a random-loss mask.
- FlexFEC protects the first simulcast layer only, publishing with
  `simulcast: false` is recommended for protected tracks.
- FEC spends bandwidth proactively: it pays off on links where loss is
  bursty or the RTT makes retransmissions slow, and costs throughput on
  clean or low-latency links.
