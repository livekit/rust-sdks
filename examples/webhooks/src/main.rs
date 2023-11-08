use env_logger::init;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use hyper::{Method, StatusCode};
use livekit::options::TrackPublishOptions;
use livekit::track::{LocalTrack, LocalVideoTrack};
use livekit::webrtc::video_source::{native, RtcVideoSource, VideoResolution};
use livekit::{self as lsdk, RoomEvent};
use livekit_api::access_token::{AccessToken, TokenVerifier, VideoGrants};
use livekit_api::webhooks;
use log::{info, warn};
use lsdk::options::VideoCodec;
use lsdk::track::TrackSource;
use std::convert::Infallible;
use std::net::SocketAddr;

const BOT_NAME : &str ="donut";

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    pretty_env_logger::formatted_builder()
        .filter_module("hyper", log::LevelFilter::Info)
        .filter_module("tokio", log::LevelFilter::Info)
        .filter_module("webhooks", log::LevelFilter::Info)
        .init();

    let addr = SocketAddr::from(([127, 0, 0, 1], 6669));

    let make_svc = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle)) });
    let server = Server::bind(&addr).serve(make_svc);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

async fn handle(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/lsdk-webhook") => {
            info!("POST request received 🎉");
            let token_verifier = TokenVerifier::new().unwrap();
            let webhook_receiver = webhooks::WebhookReceiver::new(token_verifier);

            let jwt = req
                .headers()
                .get("Authorization")
                .and_then(|hv| hv.to_str().ok())
                .unwrap_or_default()
                .to_string();

            let jwt = jwt.trim();

            let body = hyper::body::to_bytes(req.into_body()).await.unwrap();
            let body = std::str::from_utf8(&body).unwrap();

            let res = webhook_receiver.receive(&body, &jwt);
            if let Ok(event) = res {
                println!("Received event: {:?}", event);
                if event.event == "room_started" {
                    info!("ROOM STARTED 🎉");
                    let livekit_protocol::Room {
                        name: room_name,
                        max_participants,
                        num_participants,
                        ..
                    } = event.room.unwrap();
                    if num_participants < max_participants {
                        let lvkt_url =
                            std::env::var("LIVEKIT_WS_URL").expect("LIVEKIT_WS_URL is not set");
                        let lvkt_token = match create_token(room_name, BOT_NAME) {
                            Ok(i) => i,
                            Err(_e) => {
                                return Ok(Response::builder()
                                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                                    .body(Body::from("Couldn't create token for bot"))
                                    .unwrap())
                            }
                        };

                        let (room, rx) = match lsdk::Room::connect(
                            &lvkt_url,
                            &lvkt_token,
                            lsdk::RoomOptions::default(),
                        )
                        .await
                        {
                            Ok(i) => i,
                            Err(_) => {
                                return Ok(Response::builder()
                                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                                    .body(Body::from("Couldn't connect to room"))
                                    .unwrap())
                            }
                        };

                        info!("Connected to room. ID -> [{}]", room.name());

                        let (width, height) = (1920, 1080);
                        let livekit_vid_src =
                            native::NativeVideoSource::new(VideoResolution { width, height });
                        let track = LocalVideoTrack::create_video_track(
                            BOT_NAME,
                            RtcVideoSource::Native(livekit_vid_src.clone()),
                        );

                        match room
                            .local_participant()
                            .publish_track(
                                LocalTrack::Video(track),
                                TrackPublishOptions {
                                    source: TrackSource::Camera,
                                    video_codec: VideoCodec::VP8,
                                    ..Default::default()
                                },
                            )
                            .await
                        {
                            Ok(i) => i,
                            Err(_) => {
                                return Ok(Response::builder()
                                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                                    .body(Body::from("Couldn't  publish track to room"))
                                    .unwrap())
                            }
                        };

                        // Should handle room events in the background
                        tokio::spawn(handle_room_events(rx));

                        info!("\nSERVER FINISHED PROCESSING ROOM_STARTED WEBHOOK");
                    }
                }
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::from("'Room Started' Webhook Processed Successfully"))
                    .unwrap())
            } else {
                warn!("Failed to receive event: {:?}", res);
                Ok(Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(Body::from("Invalid request"))
                    .unwrap())
            }
        }
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Http method not supported"))
            .unwrap()),
    }
}

fn create_token(room_name: String, ai_name: &str) -> anyhow::Result<String> {
    let api_key = std::env::var("LIVEKIT_API_KEY")?;
    let api_secret = std::env::var("LIVEKIT_API_SECRET")?;

    let ttl = std::time::Duration::from_secs(60 * 10);
    Ok(
        AccessToken::with_api_key(api_key.as_str(), api_secret.as_str())
            .with_ttl(ttl)
            .with_identity(ai_name)
            .with_name(ai_name)
            .with_grants(VideoGrants {
                room: room_name,
                room_list: true,
                room_record: true,
                room_join: true,
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
                can_update_own_metadata: true,
                ..Default::default()
            })
            .to_jwt()?,
    )
}

async fn handle_room_events(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
) -> anyhow::Result<()> {
    while let Some(msg) = rx.recv().await {
        info!("\n.............. incoming msg {:?}", msg);
    }
    warn!("\nNO LONGER HANDLING ROOM EVENTS");
    Ok(())
}
