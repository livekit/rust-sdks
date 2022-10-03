use livekit::proto::data_packet;
use livekit::room;

const URL: &str = "ws://localhost:7880";
const TOKEN : &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE2NjgxMzc0NDgsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJ3ZWIiLCJuYmYiOjE2NjQ1Mzc0NDgsInN1YiI6IndlYiIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.6VMDdXJYrW3KWrEzxx4hzbmMQnjQIRILQ48Qrbx5j44";

// eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE2NjgxMzc0NDgsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJ3ZWIiLCJuYmYiOjE2NjQ1Mzc0NDgsInN1YiI6IndlYiIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.6VMDdXJYrW3KWrEzxx4hzbmMQnjQIRILQ48Qrbx5j44

#[tokio::main]
async fn main() -> Result<(), room::RoomError> {
    tracing_subscriber::fmt::init();

    let mut room = room::connect(URL, TOKEN).await?;
    room.local_participant()
        .publish_data(b"this is a test", data_packet::Kind::Reliable)
        .await?;

    Ok(())
}
