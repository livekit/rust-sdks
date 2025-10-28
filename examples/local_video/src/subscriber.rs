use anyhow::Result;
use clap::Parser;
use eframe::egui;
use futures::StreamExt;
use livekit::prelude::*;
use libwebrtc::prelude::VideoBuffer;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit_api::access_token;
use log::{debug, info};
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// LiveKit participant identity
    #[arg(long, default_value = "rust-video-subscriber")] 
    identity: String,

    /// LiveKit room name
    #[arg(long, default_value = "video-room")] 
    room_name: String,

    /// LiveKit server URL
    #[arg(long)]
    url: Option<String>,

    /// LiveKit API key
    #[arg(long)]
    api_key: Option<String>,

    /// LiveKit API secret
    #[arg(long)]
    api_secret: Option<String>,
}

struct SharedFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    dirty: bool,
}

struct VideoApp {
    shared: Arc<Mutex<SharedFrame>>,
    texture: Option<egui::TextureHandle>,
}

impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut shared = self.shared.lock();
            if shared.dirty {
                let size = [shared.width as usize, shared.height as usize];
                let image = egui::ColorImage::from_rgba_unmultiplied(size, &shared.rgba);
                match &mut self.texture {
                    Some(tex) => tex.set(image, egui::TextureOptions::LINEAR),
                    None => {
                        self.texture = Some(ui.ctx().load_texture(
                            "remote-video",
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
                shared.dirty = false;
            }

            if let Some(tex) = &self.texture {
                let tex_size = tex.size_vec2();
                let available = ui.available_size();
                let scale = (available.x / tex_size.x).min(available.y / tex_size.y).max(1.0);
                let desired = tex_size * scale;
                ui.image((tex.id(), desired));
            } else {
                ui.heading("Waiting for video...");
            }
        });

        ctx.request_repaint();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    // LiveKit connection details
    let url = std::env::var("LIVEKIT_URL").ok().or(args.url).expect(
        "LIVEKIT_URL must be provided via --url or env",
    );
    let api_key = std::env::var("LIVEKIT_API_KEY").ok().or(args.api_key).expect(
        "LIVEKIT_API_KEY must be provided via --api-key or env",
    );
    let api_secret = std::env::var("LIVEKIT_API_SECRET").ok().or(args.api_secret).expect(
        "LIVEKIT_API_SECRET must be provided via --api-secret or env",
    );

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: args.room_name.clone(),
            can_subscribe: true,
            ..Default::default()
        })
        .to_jwt()?;

    info!("Connecting to LiveKit room '{}' as '{}'...", args.room_name, args.identity);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    let (room, _) = Room::connect(&url, &token, room_options).await?;
    let room = Arc::new(room);
    info!("Connected: {} - {}", room.name(), room.sid().await);

    // Shared frame buffer for UI
    let shared = Arc::new(Mutex::new(SharedFrame { width: 0, height: 0, rgba: Vec::new(), dirty: false }));

    // Subscribe to room events: on first video track, start sink task
    let shared_clone = shared.clone();
    let rt = tokio::runtime::Handle::current();
    tokio::spawn(async move {
        let mut events = room.subscribe();
        while let Some(evt) = events.recv().await {
            if let RoomEvent::TrackSubscribed { track, .. } = evt {
                if let livekit::track::RemoteTrack::Video(video_track) = track {
                    info!("Subscribed to video track: {}", video_track.name());
                    // Start background sink thread
                    let shared2 = shared_clone.clone();
                    std::thread::spawn(move || {
                        let mut sink = NativeVideoStream::new(video_track.rtc_track());
                        while let Some(frame) = rt.block_on(sink.next()) {
                            let buffer = frame.buffer.to_i420();
                            let w = buffer.width();
                            let h = buffer.height();

                            let (sy, su, sv) = buffer.strides();
                            let (dy, du, dv) = buffer.data();

                            let mut rgba = vec![0u8; (w * h * 4) as usize];
                            libwebrtc::native::yuv_helper::i420_to_rgba(
                                dy, sy, du, su, dv, sv, &mut rgba, w * 4, w as i32, h as i32,
                            );

                            let mut s = shared2.lock();
                            s.width = w;
                            s.height = h;
                            s.rgba = rgba;
                            s.dirty = true;
                        }
                    });
                    break;
                }
            }
        }
    });

    // Start UI
    let app = VideoApp { shared, texture: None };
    let native_options = eframe::NativeOptions::default();
    eframe::run_native("LiveKit Video Subscriber", native_options, Box::new(|_| Ok::<Box<dyn eframe::App>, _>(Box::new(app))))?;

    Ok(())
}


