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
  These request repair-to-media packet ratios of 0, 0.15, 0.25, and 0.35. The
  default is `Disabled`, which sends no repair packets.
- Protection is applied through libwebrtc's `RtpParameters::fec_protection` in
  fixed mode: each frame is protected on its own, independent of observed loss.
  Repair packets are rounded up per frame, so small frames carry more overhead
  than the requested ratio.
- libwebrtc does not send FlexFEC for simulcast senders, so publish with
  `simulcast: false` for protected tracks.
- FEC spends bandwidth proactively: it pays off on links where loss is
  bursty or the RTT makes retransmissions slow, and costs throughput on
  clean or low-latency links.
