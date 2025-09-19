#[cfg(feature = "__lk-e2e-test")]
use {
    crate::common::test_rooms_with_options,
    anyhow::{anyhow, Result},
    livekit::{
        e2ee::key_provider::{KeyProvider, KeyProviderOptions},
        DataPacket, E2eeOptions, RoomEvent, RoomOptions, SimulateScenario,
    },
    std::{sync::Arc, time::Duration},
    tokio::{sync::oneshot, time},
};

mod common;

// These tests depend on a LiveKit server, and thus are not enabled by default;
// to run them, start a local LiveKit server in development mode, and enable the
// E2E test feature:
//
// > livekit-server --dev
// > cargo test --features __lk-e2e-test
//
#[cfg(feature = "__lk-e2e-test")]
#[tokio::test]
async fn test_reliable_retry_e2ee() -> Result<()> {
    const ITERATIONS: usize = 128;
    const PAYLOAD_SIZE: usize = 4096;

    let key_provider_1 = KeyProvider::new(KeyProviderOptions::default());
    let key_provider_2 = KeyProvider::new(KeyProviderOptions::default());

    key_provider_1.set_shared_key("password".as_bytes().to_vec(), 0);
    key_provider_2.set_shared_key("password".as_bytes().to_vec(), 0);

    // Set up test rooms
    let mut options1 = RoomOptions::default();
    options1.encryption = Some(E2eeOptions {
        key_provider: key_provider_1,
        encryption_type: livekit::e2ee::EncryptionType::Gcm,
    });

    let mut options2 = RoomOptions::default();
    options2.encryption = Some(E2eeOptions {
        key_provider: key_provider_2,
        encryption_type: livekit::e2ee::EncryptionType::Gcm,
    });

    let options = vec![options1, options2];
    let mut rooms = test_rooms_with_options(options).await?;
    let (sending_room, _) = rooms.pop().unwrap();
    let (receiving_room, mut receiving_event_rx) = rooms.pop().unwrap();

    sending_room.e2ee_manager().set_enabled(true);
    receiving_room.e2ee_manager().set_enabled(true);

    let sending_room = Arc::new(sending_room);
    let receiving_room = Arc::new(receiving_room);

    let receiving_identity = receiving_room.local_participant().identity();
    let (fulfill, expectation) = oneshot::channel();

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
