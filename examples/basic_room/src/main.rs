use livekit::{prelude::*, options::video};
use std::env;
use futures::StreamExt;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit::webrtc::encoded_video_frame_stream::native::NativeEncodedVideoFrameStream;
use livekit::webrtc::encoded_video_frame::EncodedVideoFrame;

use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::encoded_audio_frame_stream::native::NativeEncodedAudioFrameStream;
use livekit::webrtc::encoded_audio_frame::EncodedAudioFrame;

// Basic demo to connect to a room using the specified env variables

#[tokio::main]
async fn main() {
    // let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    // let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let url: &str = "wss://lighttwist.livekit.cloud";
    // cyberpunk 573ae795-c53f-47dd-a195-ab11148f9416
    //let token : &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE3MTM5MDU4OTYsImlzcyI6IkFQSU5YTVNtTDlWUjVxNiIsIm5hbWUiOiJ1c2VyIiwibmJmIjoxNjg1MTA1ODk2LCJzdWIiOiJ1c2VyIiwidmlkZW8iOnsicm9vbSI6IjU3M2FlNzk1LWM1M2YtNDdkZC1hMTk1LWFiMTExNDhmOTQxNiIsInJvb21Kb2luIjp0cnVlfX0.yHlsWep5RN-JjZJMtZ_iZRA7sVnq2RfJLLFRge0bUEQ";

    // b8239902
    let token : &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE3MTQ4NDM5NDEsImlzcyI6IkFQSU5YTVNtTDlWUjVxNiIsIm5hbWUiOiJ1c2VyIiwibmJmIjoxNjg2MDQzOTQxLCJzdWIiOiJ1c2VyIiwidmlkZW8iOnsicm9vbSI6ImI4MzI5OTAyLTJmNjMtNGM2Ny04MzNlLTA4MWE5NmY2MmVkMyIsInJvb21Kb2luIjp0cnVlfX0.NOjND8jsn7LM7_UGRqZWLWzqK6HAqcD4ncTIGkf6I5U";

    

    let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default())
        .await
        .unwrap();
    log::info!("Connected to room: {} - {}", room.name(), room.sid());


    room.local_participant()
        .publish_data(
            "Hello world".to_owned().into_bytes(),
            DataPacketKind::Reliable,
            Default::default(),
        )
        .await
        .unwrap();

    while let Some(msg) = rx.recv().await {
        println!("Event: {:?}", msg);
        match msg {
            RoomEvent::TrackSubscribed { track, publication, participant } => {
                if let RemoteTrack::Video(video_track) = &track {
                    match &video_track.receiver() {
                        Some(receiver) => {
                            let parameters = receiver.parameters();
                            println!("Parameters: {:?}", parameters);
                            let mut encoded_frame_stream = NativeEncodedVideoFrameStream::new(receiver);
                            while let Some(frame) = encoded_frame_stream.next().await {
                                println!("Got encoded frame - {}x{} type: {}", frame.width(), frame.height(), frame.payload_type());
                            //     let payload = frame.payload();
                            //     println!("payload:");
                            //     for b in payload {
                            //         print!("{:02x}", b);
                            //     }
                            //     println!();
                            }                            
                        },
                        None => {
                            println!("No receiver!");
                        },
                    }
                    
                    // let rtc_track = video_track.rtc_track();
                    // let mut video_stream = NativeVideoStream::new(rtc_track);
                    // while let Some(frame) = video_stream.next().await {

                    // }
                    // break;
                }
                else if let RemoteTrack::Audio(audio_track) = &track {
                    // match &audio_track.receiver() {
                    //     Some(receiver) => {
                    //         let parameters = receiver.parameters();
                    //         // println!("Parameters: {:?}", parameters);
                    //         let mut encoded_frame_stream = NativeEncodedAudioFrameStream::new(receiver);
                    //         while let Some(frame) = encoded_frame_stream.next().await {
                    //             println!("Got encoded audio frame type: {}", frame.payload_type());
                    //             let payload = frame.payload();
                    //             println!("payload:");
                    //             for b in payload {
                    //                 print!("{:02x}", b);
                    //             }
                    //             println!();
                    //         }
                    //         println!("Exited");
                    //     },
                    //     None => {
                    //         println!("No receiver!");
                    //     },
                    // }
                }
            },
            _ => {}
        }
    }
}
