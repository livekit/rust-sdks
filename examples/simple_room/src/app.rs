use crate::events::UiCmd;
use crate::video_renderer::VideoRenderer;
use crate::{events::AsyncCmd, video_grid::VideoGrid};
use egui::{Rounding, Stroke};
use egui_wgpu::WgpuConfiguration;
use image::ImageFormat;
use livekit::options::{TrackPublishOptions, VideoCaptureOptions};
use livekit::prelude::*;
use livekit::webrtc::native::yuv_helper;
use livekit::webrtc::video_frame::native::I420BufferExt;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::SimulateScenario;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

// Useful default constants for developing
const DEFAULT_URL: &str = "ws://localhost:7880";
const DEFAULT_TOKEN : &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE5MDY2MTMyODgsImlzcyI6IkFQSVRzRWZpZFpqclFvWSIsIm5hbWUiOiJuYXRpdmUiLCJuYmYiOjE2NzI2MTMyODgsInN1YiI6Im5hdGl2ZSIsInZpZGVvIjp7InJvb20iOiJ0ZXN0Iiwicm9vbUFkbWluIjp0cnVlLCJyb29tQ3JlYXRlIjp0cnVlLCJyb29tSm9pbiI6dHJ1ZSwicm9vbUxpc3QiOnRydWV9fQ.uSNIangMRu8jZD5mnRYoCHjcsQWCrJXgHCs0aNIgBFY";

// eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE5MDY2MTM0MzcsImlzcyI6IkFQSVRzRWZpZFpqclFvWSIsIm5hbWUiOiJ3ZWIiLCJuYmYiOjE2NzI2MTM0MzcsInN1YiI6IndlYiIsInZpZGVvIjp7InJvb20iOiJ0ZXN0Iiwicm9vbUFkbWluIjp0cnVlLCJyb29tQ3JlYXRlIjp0cnVlLCJyb29tSm9pbiI6dHJ1ZSwicm9vbUxpc3QiOnRydWV9fQ.DFTXt60n1kzGq4cSuOhbFBTQW2nd3rlcXKQ54sXsP8s

use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::{WindowBuilder, WindowId},
};

struct AppState {
    room: Mutex<Option<Room>>,
    close_tx: Mutex<Option<oneshot::Sender<()>>>,
    connecting: AtomicBool,
}

struct App {
    state: Arc<AppState>,

    video_renderers: HashMap<(ParticipantSid, TrackSid), VideoRenderer>,
    egui_context: egui::Context,
    egui_state: egui_winit::State,
    egui_painter: egui_wgpu::winit::Painter,
    window: winit::window::Window,
    cmd_tx: mpsc::UnboundedSender<AsyncCmd>,
    cmd_rx: mpsc::UnboundedReceiver<UiCmd>,
    // UI State
    lk_url: String,
    lk_token: String,
    connection_failure: Option<String>,
    room_state: ConnectionState,
}

pub fn run(rt: tokio::runtime::Runtime) {
    rt.block_on(async {
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_title("LiveKit - NativeSDK")
            .build(&event_loop)
            .unwrap();

        let egui_context = egui::Context::default();
        let egui_state = egui_winit::State::new(&event_loop);
        let mut egui_painter = egui_wgpu::winit::Painter::new(WgpuConfiguration::default(), 1, 32);

        unsafe {
            egui_painter.set_window(Some(&window));
        }

        let (async_cmd_tx, mut async_cmd_rx) = mpsc::unbounded_channel::<AsyncCmd>();
        let (ui_cmd_tx, ui_cmd_rx) = mpsc::unbounded_channel::<UiCmd>();

        let state = Arc::new(AppState {
            room: Default::default(),
            close_tx: Default::default(),
            connecting: AtomicBool::new(false),
        });

        let mut app = App {
            state: state.clone(),
            video_renderers: HashMap::default(),
            egui_context,
            egui_state,
            egui_painter,
            window,
            cmd_tx: async_cmd_tx,
            cmd_rx: ui_cmd_rx,
            lk_url: DEFAULT_URL.to_owned(),
            lk_token: DEFAULT_TOKEN.to_owned(),
            connection_failure: None,
            room_state: ConnectionState::Connected,
        };

        // Async event loop
        tokio::spawn(async move {
            while let Some(event) = async_cmd_rx.recv().await {
                match event {
                    AsyncCmd::RoomConnect { url, token } => {
                        if let Some(close_tx) = state.close_tx.lock().take() {
                            let _ = state.room.lock().take().unwrap().close().await;
                            let _ = close_tx.send(());
                        }

                        state.connecting.store(true, Ordering::SeqCst);

                        let res = Room::connect(&url, &token).await;
                        match res {
                            Ok((room, room_events)) => {
                                let (close_tx, close_rx) = oneshot::channel();
                                state.room.lock().replace(room);
                                state.close_tx.lock().replace(close_tx);

                                tokio::spawn(room_task(
                                    state.clone(),
                                    room_events,
                                    close_rx,
                                    ui_cmd_tx.clone(),
                                ));

                                let _ = ui_cmd_tx.send(UiCmd::ConnectResult { result: Ok(()) });
                            }
                            Err(err) => {
                                let _ = ui_cmd_tx.send(UiCmd::ConnectResult { result: Err(err) });
                            }
                        }

                        state.connecting.store(false, Ordering::SeqCst);
                    }
                    AsyncCmd::RoomDisconnect => {
                        if let Some(close_tx) = state.close_tx.lock().take() {
                            let _ = state.room.lock().take().unwrap().close().await;
                            let _ = close_tx.send(());
                        }
                    }
                    AsyncCmd::SimulateScenario { scenario } => {
                        if let Some(room) = state.room.lock().as_ref() {
                            let _ = room.session().simulate_scenario(scenario).await;
                        }
                    }
                }
            }
        });

        tokio::task::block_in_place(move || loop {
            // ui/main thread
            event_loop.run(move |event, _, control_flow| {
                app.update(event, control_flow);
            });
        });
    });
}

async fn room_task(
    _app_state: Arc<AppState>,
    mut room_events: mpsc::UnboundedReceiver<RoomEvent>,
    mut close_rx: oneshot::Receiver<()>,
    ui_cmd_tx: mpsc::UnboundedSender<UiCmd>,
) {
    loop {
        tokio::select! {
            Some(event) = room_events.recv() => {
                let _ = ui_cmd_tx.send(UiCmd::RoomEvent{event});
            }
            _ = &mut close_rx => {
                break;
            }
        }
    }
}

impl App {
    fn update<T>(&mut self, event: Event<'_, T>, control_flow: &mut ControlFlow) {
        if let Ok(cmd) = self.cmd_rx.try_recv() {
            match cmd {
                UiCmd::ConnectResult { result } => {
                    if let Err(err) = result {
                        self.connection_failure = Some(err.to_string());
                    } else {
                        self.connection_failure = None
                    }
                }
                UiCmd::RoomEvent { event } => {
                    match event {
                        RoomEvent::TrackSubscribed {
                            track, participant, ..
                        } => {
                            match track.clone() {
                                RemoteTrack::Video(video_track) => {
                                    // Create a new VideoRenderer
                                    let video_renderer = VideoRenderer::new(
                                        self.egui_painter.render_state().clone().unwrap(),
                                        video_track.rtc_track(),
                                    );
                                    self.video_renderers
                                        .insert((participant.sid(), track.sid()), video_renderer);
                                }
                                RemoteTrack::Audio(_) => {
                                    // The demo doesn't support Audio rendering at the moment.
                                }
                            };
                        }
                        RoomEvent::TrackUnsubscribed {
                            track, participant, ..
                        } => {
                            self.video_renderers
                                .remove(&(participant.sid(), track.sid()));
                        }
                        _ => {}
                    }
                }
            }
        }

        match event {
            Event::WindowEvent { window_id, event } => {
                if let Some(flow) = self.on_window_event(window_id, event) {
                    *control_flow = flow;
                }
            }
            Event::RedrawRequested(window_id) if window_id == self.window.id() => {
                self.render();
            }
            Event::RedrawEventsCleared => {
                self.window.request_redraw();
            }
            _ => {}
        };
    }

    fn on_window_event(
        &mut self,
        _window_id: WindowId,
        event: WindowEvent<'_>,
    ) -> Option<ControlFlow> {
        if self
            .egui_state
            .on_event(&self.egui_context, &event)
            .consumed
        {
            return None;
        }

        match event {
            WindowEvent::CloseRequested => Some(ControlFlow::Exit),
            WindowEvent::Resized(inner_size) => {
                self.egui_painter
                    .on_window_resized(inner_size.width, inner_size.height);
                None
            }
            WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                self.egui_painter
                    .on_window_resized(new_inner_size.width, new_inner_size.height);
                None
            }
            _ => None,
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::TopBottomPanel::top("top_panel").show(ui.ctx(), |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Tools", |ui| {
                    if ui.button("Logs").clicked() {}
                    if ui.button("Profiler").clicked() {}
                    if ui.button("WebRTC Stats").clicked() {}
                    if ui.button("Events").clicked() {}
                });
                ui.menu_button("Simulate", |ui| {
                    if ui.button("SignalReconnect").clicked() {
                        let _ = self.cmd_tx.send(AsyncCmd::SimulateScenario {
                            scenario: SimulateScenario::SignalReconnect,
                        });
                    }
                    if ui.button("Speaker").clicked() {
                        let _ = self.cmd_tx.send(AsyncCmd::SimulateScenario {
                            scenario: SimulateScenario::Speaker,
                        });
                    }
                    if ui.button("NodeFailure").clicked() {
                        let _ = self.cmd_tx.send(AsyncCmd::SimulateScenario {
                            scenario: SimulateScenario::NodeFailure,
                        });
                    }
                    if ui.button("ServerLeave").clicked() {
                        let _ = self.cmd_tx.send(AsyncCmd::SimulateScenario {
                            scenario: SimulateScenario::ServerLeave,
                        });
                    }
                    if ui.button("Migration").clicked() {
                        let _ = self.cmd_tx.send(AsyncCmd::SimulateScenario {
                            scenario: SimulateScenario::Migration,
                        });
                    }
                    if ui.button("ForceTcp").clicked() {
                        let _ = self.cmd_tx.send(AsyncCmd::SimulateScenario {
                            scenario: SimulateScenario::ForceTcp,
                        });
                    }
                    if ui.button("ForceTls").clicked() {
                        let _ = self.cmd_tx.send(AsyncCmd::SimulateScenario {
                            scenario: SimulateScenario::ForceTls,
                        });
                    }
                });

                ui.menu_button("Publish", |ui| {
                    if ui.button("CustomTrack - LK Logo").clicked() {
                        self.publish_track();
                    }
                });
            });
        });

        egui::SidePanel::right("room_panel")
            .default_width(256.0)
            .show(ui.ctx(), |ui| {
                ui.heading("Livekit - Connect to a room");
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("URL: ");
                    ui.text_edit_singleline(&mut self.lk_url);
                });

                ui.horizontal(|ui| {
                    ui.label("Token: ");
                    ui.text_edit_singleline(&mut self.lk_token);
                });

                ui.horizontal(|ui| {
                    let connecting = self.state.connecting.load(Ordering::SeqCst);

                    let room = self.state.room.lock();
                    ui.add_enabled_ui(!connecting && room.is_none(), |ui| {
                        if ui.button("Connect").clicked() {
                            self.connection_failure = None;
                            let _ = self.cmd_tx.send(AsyncCmd::RoomConnect {
                                url: self.lk_url.clone(),
                                token: self.lk_token.clone(),
                            });
                        }
                    });

                    if connecting {
                        ui.spinner();
                    }

                    if room.is_some() {
                        if ui.button("Disconnect").clicked() {
                            let _ = self.cmd_tx.send(AsyncCmd::RoomDisconnect);
                        }
                    }
                });

                if let Some(err) = &self.connection_failure {
                    ui.colored_label(egui::Color32::RED, err);
                }

                ui.separator();

                {
                    // Room Info
                    let room = self.state.room.lock();
                    if let Some(room) = room.as_ref() {
                        ui.label(format!("Name: {}", room.session().name()));
                        ui.label(format!("SID: {}", room.session().sid()));
                        ui.label(format!(
                            "ConnectionState: {:?}",
                            room.session().connection_state()
                        ));
                        ui.label(format!(
                            "ParticipantCount: {:?}",
                            room.session().participants().len() + 1
                        ));
                    }
                }
            });

        egui::CentralPanel::default().show(ui.ctx(), |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                VideoGrid::new("default_grid")
                    .max_columns(6)
                    .show(ui, |ui| {
                        if self.room_state == ConnectionState::Disconnected {
                            for _ in 0..20 {
                                ui.video_frame(|ui| {
                                    egui::Frame::none().fill(egui::Color32::DARK_GRAY).show(
                                        ui,
                                        |ui| {
                                            ui.allocate_space(ui.available_size());
                                        },
                                    );
                                });
                            }
                        } else {
                            // Render participant videos
                            for ((participant_sid, _), video_renderer) in &self.video_renderers {
                                ui.video_frame(|ui| {
                                    let rect = ui.available_rect_before_wrap();
                                    ui.painter().rect(
                                        rect,
                                        Rounding::none(),
                                        egui::Color32::DARK_GRAY,
                                        Stroke::NONE,
                                    );

                                    if let Some(tex) = video_renderer.texture_id() {
                                        ui.painter().image(
                                            tex,
                                            rect,
                                            egui::Rect::from_min_max(
                                                egui::pos2(0.0, 0.0),
                                                egui::pos2(1.0, 1.0),
                                            ),
                                            egui::Color32::WHITE,
                                        );
                                    }

                                    let name = self.state.room.lock().as_ref().and_then(|room| {
                                        room.session()
                                            .participants()
                                            .get(participant_sid)
                                            .map(|p| p.name())
                                    });

                                    if let Some(name) = name {
                                        ui.painter().text(
                                            egui::pos2(rect.min.x + 5.0, rect.max.y - 5.0),
                                            egui::Align2::LEFT_BOTTOM,
                                            name,
                                            egui::FontId::default(),
                                            egui::Color32::WHITE,
                                        );
                                    }
                                });
                            }
                        }
                    });
            });
        });
    }

    fn render(&mut self) {
        self.egui_state
            .set_pixels_per_point(egui_winit::native_pixels_per_point(&self.window));

        let raw_inputs = self.egui_state.take_egui_input(&self.window);
        let full_output = self.egui_context.clone().run(raw_inputs, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.ui(ui);
            });
        });
        let clipped_primitives = self.egui_context.tessellate(full_output.shapes);

        self.egui_painter.paint_and_update_textures(
            egui_winit::native_pixels_per_point(&self.window),
            egui::Rgba::BLACK,
            &clipped_primitives,
            &full_output.textures_delta,
        );

        self.egui_state.handle_platform_output(
            &self.window,
            &self.egui_context,
            full_output.platform_output,
        );
    }

    fn publish_track(&self) {
        let session = {
            let room = self.state.room.lock();
            let Some(room) = room.as_ref() else { return; };
            room.session()
        };

        tokio::spawn(async move {
            let source = NativeVideoSource::default();
            let track = LocalVideoTrack::create_video_track(
                "livekit_logo",
                VideoCaptureOptions::default(),
                source.clone(),
            );

            session
                .local_participant()
                .publish_track(
                    LocalTrack::Video(track),
                    TrackPublishOptions {
                        simulcast: false,
                        ..Default::default()
                    },
                )
                .await
                .unwrap();

            // Load logo data
            let image = tokio::task::spawn_blocking(|| {
                image::load_from_memory_with_format(
                    include_bytes!("moving-logo.png"),
                    ImageFormat::Png,
                )
                .unwrap()
                .to_rgb8()
            })
            .await
            .unwrap();

            let logo_width = image.width() as usize;
            let logo_height = image.height() as usize;
            let image = image.into_raw();

            // Move the logo
            const MOVE_SPEED: i32 = 1;
            const FB_WIDTH: usize = 1280; // The logo can't be bigger
            const FB_HEIGHT: usize = 720;

            let mut interval = tokio::time::interval(Duration::from_millis(1000 / 30));
            let (mut dir_x, mut dir_y) = (1i32, 0i32);
            let (mut x, mut y) = (0u32, 0u32);

            // allocate buffers once
            let mut framebuffer = vec![0u32; FB_WIDTH * FB_HEIGHT];
            let mut i420_frame = VideoFrame {
                rotation: VideoRotation::VideoRotation0,
                buffer: I420Buffer::new(FB_WIDTH as u32, FB_HEIGHT as u32),
            };

            loop {
                interval.tick().await;
                /*x = (x as i32 + dir_x * MOVE_SPEED) as u32;
                y = (y as i32 + dir_y * MOVE_SPEED) as u32;

                if x >= (FB_WIDTH - logo_width) as u32 {
                    dir_x = -1;
                } else if x <= 0 {
                    dir_x = 1;
                }

                if y >= (FB_HEIGHT - logo_height) as u32 {
                    dir_y = -1;
                } else if y <= 0 {
                    dir_y = 1;
                }

                for i in 0..logo_height {
                    let row_start = x as usize + (i * FB_WIDTH);
                    let row_end = row_start + logo_width;
                    framebuffer[row_start..row_end]
                        .clone_from_slice(&image[i * logo_width..i * logo_width + logo_width]);
                }*/

                framebuffer.fill(0xff0000ff);

                let stride_y = i420_frame.buffer.stride_y();
                let stride_u = i420_frame.buffer.stride_u();
                let stride_v = i420_frame.buffer.stride_v();
                let (data_y, data_u, data_v) = i420_frame.buffer.data_mut();

                let u8_slice = unsafe {
                    std::slice::from_raw_parts(
                        framebuffer.as_ptr() as *const u8,
                        FB_HEIGHT * FB_WIDTH * 4,
                    )
                };

                yuv_helper::argb_to_i420(
                    u8_slice,
                    (FB_WIDTH * 4) as i32,
                    data_y,
                    stride_y,
                    data_u,
                    stride_u,
                    data_v,
                    stride_v,
                    FB_WIDTH as i32,
                    FB_HEIGHT as i32,
                )
                .unwrap();

                source.capture_frame(&i420_frame);
            }
        });
    }
}
