// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[cfg(feature = "__lk-e2e-test")]
use {
    crate::common::test_rooms,
    anyhow::{Ok, Result},
    livekit::{DataPacket, RoomEvent, SimulateScenario},
    std::{sync::Arc, time::Duration},
    tokio::{
        time::{self, timeout},
        try_join,
    },
};

mod common;

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_reliable_retry() -> Result<()> {
    use anyhow::Context;

    const ITERATIONS: usize = 128;
    const PAYLOAD_SIZE: usize = 4096;

    let mut rooms = test_rooms(2).await?;
    let (receiving_room, mut receiving_event_rx) = rooms.pop().unwrap();
    let (sending_room, _) = rooms.pop().unwrap();

    let receiving_identity = receiving_room.local_participant().identity();

    let sending_room = Arc::new(sending_room);
    tokio::spawn({
        let sending_room = sending_room.clone();
        async move {
            time::sleep(Duration::from_millis(200)).await;
            _ = sending_room.simulate_scenario(SimulateScenario::SignalReconnect).await;
            log::info!("Reconnecting sending room");
        }
    });

    tokio::spawn(async move {
        time::sleep(Duration::from_millis(400)).await;
        _ = receiving_room.simulate_scenario(SimulateScenario::SignalReconnect).await;
        log::info!("Reconnecting receiving room");
    });

    let send_packets = async move {
        for _ in 0..ITERATIONS {
            let packet = DataPacket {
                reliable: true,
                payload: [0xFA; PAYLOAD_SIZE].to_vec(),
                destination_identities: vec![receiving_identity.clone()],
                ..Default::default()
            };
            sending_room.local_participant().publish_data(packet).await?;
            time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    };

    let receive_packets = async move {
        let mut packets_received = 0;
        while let Some(event) = receiving_event_rx.recv().await {
            let RoomEvent::DataReceived { .. } = event else { continue };
            packets_received += 1;
            if packets_received == ITERATIONS {
                break;
            }
        }
        Ok(())
    };
    timeout(Duration::from_secs(15), async { try_join!(send_packets, receive_packets) })
        .await?
        .context("Not all packets received before timeout")?;
    Ok(())
}

/// A data message larger than the negotiated SCTP max message size must be
/// rejected with an error by `publish_data` (rather than silently aborting the
/// data channel), and the channel must keep working afterwards.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_oversized_data_message_rejected() -> Result<()> {
    use anyhow::{ensure, Context};

    // Comfortably larger than any plausible negotiated max-message-size
    // (LiveKit/pion advertises ~64 KiB; libwebrtc's default is 256 KiB).
    const OVERSIZED: usize = 300_000;
    const SMALL: usize = 256;

    let mut rooms = test_rooms(2).await?;
    let (receiving_room, mut receiving_event_rx) = rooms.pop().unwrap();
    let (sending_room, _) = rooms.pop().unwrap();
    let receiving_identity = receiving_room.local_participant().identity();

    let small_packet = || DataPacket {
        reliable: true,
        payload: vec![0xAB; SMALL],
        destination_identities: vec![receiving_identity.clone()],
        ..Default::default()
    };

    // Baseline: a small reliable packet publishes successfully.
    sending_room.local_participant().publish_data(small_packet()).await?;

    // The oversized packet must be rejected with an error and must NOT be sent
    // to libwebrtc (which would abruptly close the data channel).
    let result = sending_room
        .local_participant()
        .publish_data(DataPacket {
            reliable: true,
            payload: vec![0xCD; OVERSIZED],
            destination_identities: vec![receiving_identity.clone()],
            ..Default::default()
        })
        .await;
    ensure!(
        result.is_err(),
        "oversized publish_data should return an error, got {:?}",
        result
    );

    // The channel must still be usable: this follow-up small packet proves the
    // oversized send did not break publishing (no 15s publisher timeout).
    sending_room.local_participant().publish_data(small_packet()).await?;

    // Exactly the two small packets should arrive; the oversized one never does.
    let receive = async {
        let mut received = 0;
        while let Some(event) = receiving_event_rx.recv().await {
            if let RoomEvent::DataReceived { payload, .. } = event {
                ensure!(payload.len() == SMALL, "unexpected packet size: {}", payload.len());
                received += 1;
                if received == 2 {
                    break;
                }
            }
        }
        Ok(())
    };
    timeout(Duration::from_secs(15), receive)
        .await
        .context("did not receive both small packets before timeout")??;
    Ok(())
}
