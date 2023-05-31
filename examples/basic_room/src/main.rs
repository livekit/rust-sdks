use livekit::{prelude::*, options::video};
use std::env;
use futures::StreamExt;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit::webrtc::encoded_frame_stream::native::NativeEncodedFrameStream;

// Basic demo to connect to a room using the specified env variables

#[tokio::main]
async fn main() {
    // let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    // let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let url: &str = "wss://lighttwist.livekit.cloud";
    // cyberpunk 573ae795-c53f-47dd-a195-ab11148f9416
    let token : &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE3MTM5MDU4OTYsImlzcyI6IkFQSU5YTVNtTDlWUjVxNiIsIm5hbWUiOiJ1c2VyIiwibmJmIjoxNjg1MTA1ODk2LCJzdWIiOiJ1c2VyIiwidmlkZW8iOnsicm9vbSI6IjU3M2FlNzk1LWM1M2YtNDdkZC1hMTk1LWFiMTExNDhmOTQxNiIsInJvb21Kb2luIjp0cnVlfX0.yHlsWep5RN-JjZJMtZ_iZRA7sVnq2RfJLLFRge0bUEQ";

    let (room, mut rx) = Room::connect(&url, &token).await.unwrap();
    let session = room.session();
    println!("Connected to room: {} - {}", session.name(), session.sid());

    while let Some(msg) = rx.recv().await {
        //println!("Event: {:?}", msg);
        match msg {
            RoomEvent::TrackSubscribed { track, publication, participant } => {
                if let RemoteTrack::Video(video_track) = &track {
                    match &video_track.receiver() {
                        Some(receiver) => {
                            // let transformer = receiver.new_adapted_frame_transformer();
                            let mut encoded_frame_stream = NativeEncodedFrameStream::new(receiver);
                            // receiver.set_depacketizer_to_decoder_frame_transformer(transformer);
                        },
                        None => {
                            println!("No transceiver!");
                        },
                    }
                    
                    let rtc_track = video_track.rtc_track();
                    let mut video_stream = NativeVideoStream::new(rtc_track);
                    //let mut frame_interceptor = EncodedFrameInterceptor::new(rtc_track);

                    while let Some(frame) = video_stream.next().await {

                    }
                    break;
                }
            },
            _ => {}
        }
    }
}
