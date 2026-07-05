use livekit_api::services::room::{CreateRoomOptions, RoomClient};

#[tokio::main]
async fn main() {
    let room_service = RoomClient::new("http://localhost:7880").unwrap();

    let room = room_service.create_room("my_room", CreateRoomOptions::default()).await.unwrap();

    println!("Created room: {:?}", room);
}
