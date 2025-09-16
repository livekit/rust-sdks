#[cfg(feature = "__lk-e2e-test")]
use {
    crate::common::test_rooms,
    anyhow::{anyhow, Result},
    livekit::{DataPacket, RoomEvent, SimulateScenario},
    std::{sync::Arc, time::Duration},
    tokio::{sync::oneshot, time},
};

mod common;

#[cfg(feature = "__lk-e2e-test")]
#[tokio::test]
async fn test_reliable_retry() -> Result<()> {
    const ITERATIONS: usize = 128;
    const PAYLOAD_SIZE: usize = 4096;

    // Set up test rooms
    let mut rooms = test_rooms(2).await?;
    let (sending_room, _) = rooms.pop().unwrap();
    let (receiving_room, mut receiving_event_rx) = rooms.pop().unwrap();

    let sending_room = Arc::new(sending_room);
    let receiving_room = Arc::new(receiving_room);

    let receiving_identity = receiving_room.local_participant().identity();
    let (fulfill, expectation) = oneshot::channel();

    tokio::spawn({
        let sending_room = sending_room.clone();
        async move {
            time::sleep(Duration::from_millis(200)).await;
            _ = sending_room.simulate_scenario(SimulateScenario::SignalReconnect).await;
            println!("Reconnecting sending room");
        }
    });
    tokio::spawn({
        let receiving_room = receiving_room.clone();
        async move {
            time::sleep(Duration::from_millis(400)).await;
            _ = receiving_room.simulate_scenario(SimulateScenario::SignalReconnect).await;
            println!("Reconnecting receiving room");
        }
    });

    tokio::spawn({
        let fulfill = fulfill;
        async move {
            let mut packets_received = 0;
            while let Some(event) = receiving_event_rx.recv().await {
                if let RoomEvent::DataReceived { payload, .. } = event {
                    assert_eq!(payload.len(), PAYLOAD_SIZE);
                    packets_received += 1;
                    if packets_received == ITERATIONS {
                        fulfill.send(()).ok();
                        break;
                    }
                }
            }
        }
    });

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

    match time::timeout(Duration::from_secs(15), expectation).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) => Err(anyhow!("Not all packets were received")),
        Err(_) => Err(anyhow!("Timed out waiting for packets")),
    }
}
