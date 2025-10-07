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
