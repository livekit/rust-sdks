use crate::events::UiCmd;
use crate::video_renderer::VideoRenderer;
use crate::{events::AsyncCmd, video_grid::VideoGrid};
use egui_wgpu::WgpuConfiguration;
use livekit::room::track::remote_track::RemoteTrackHandle;
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::mpsc;

use livekit::room::{ConnectionState, Room, RoomError, SimulateScenario};

// Useful default constants for developing
const DEFAULT_URL: &str = "ws://localhost:7880";
const DEFAULT_TOKEN : &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjIzODQ4MDY0NzMsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJuYXRpdmUiLCJuYmYiOjE2NjQ4MDY0NzMsInN1YiI6Im5hdGl2ZSIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.BgVdBnq3XFD3_BQHoe1azqjifYysubgFl6Qlzu9IQGI";

// eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjIzODQ4MDY3MzAsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJ3ZWIiLCJuYmYiOjE2NjQ4MDY3MzAsInN1YiI6IndlYiIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.VbDoULjX1CVGZu2sPy3SvWYlVZUBXxQVPmdB9BnmlN4

use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::{WindowBuilder, WindowId},
};

struct AppState {
    room: Mutex<Room>,
    connecting: AtomicBool,
}

struct App {
    state: Arc<AppState>,

    video_renderers: Vec<VideoRenderer>,
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
            room: Mutex::new(Room::new()),
            connecting: AtomicBool::new(false),
        });

        let mut app = App {
            state: state.clone(),
            video_renderers: Vec::default(),
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
            {
                let events = state.room.lock().events();
                events.on_track_subscribed({
                    let ui_cmd_tx = ui_cmd_tx.clone();
                    move |event| {
                        let ui_cmd_tx = ui_cmd_tx.clone();
                        async move {
                            ui_cmd_tx.send(UiCmd::TrackSubscribed { event }).unwrap();
                        }
                    }
                });
            }
            while let Some(event) = async_cmd_rx.recv().await {
                match event {
                    AsyncCmd::RoomConnect { url, token } => {
                        state.connecting.store(true, Ordering::SeqCst);

                        let mut room = state.room.lock();
                        ui_cmd_tx
                            .send(UiCmd::ConnectResult {
                                result: room.connect(&url, &token).await,
                            })
                            .unwrap();

                        state.connecting.store(false, Ordering::SeqCst);
                    }
                    AsyncCmd::SimulateScenario { scenario } => {
                        if let Some(handle) = state.room.lock().get_handle() {
                            let _ = handle.simulate_scenario(scenario).await;
                        }
                    }
                }
            }
        });

        tokio::task::block_in_place(move || loop {
            // UI/Main Thread
            event_loop.run(move |event, _, control_flow| {
                app.update(event, control_flow);
            });
        });
    });
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
                UiCmd::TrackSubscribed { event } => {
                    match event.track {
                        RemoteTrackHandle::Video(video_track) => {
                            // Create a new VideoRenderer
                            let video_renderer = VideoRenderer::new(
                                self.egui_painter.render_state().clone().unwrap(),
                                video_track.rtc_track(),
                            );
                            self.video_renderers.push(video_renderer);
                        }
                        RemoteTrackHandle::Audio(_) => {
                            // The demo doesn't support Audio rendering at the moment.
                        }
                    };
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
                    ui.set_enabled(!connecting);

                    if ui.button("Connect").clicked() {
                        self.connection_failure = None;
                        let _ = self.cmd_tx.send(AsyncCmd::RoomConnect {
                            url: self.lk_url.clone(),
                            token: self.lk_token.clone(),
                        });
                    }

                    if connecting {
                        ui.spinner();
                    }
                });

                if let Some(err) = &self.connection_failure {
                    ui.colored_label(egui::Color32::RED, err);
                }

                ui.separator();
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
                            for video_renderer in &self.video_renderers {
                                ui.video_frame(|ui| {
                                    if let Some(tex) = video_renderer.texture_id() {
                                        ui.painter().image(
                                            tex,
                                            ui.available_rect_before_wrap(),
                                            egui::Rect::from_min_max(
                                                egui::pos2(0.0, 0.0),
                                                egui::pos2(1.0, 1.0),
                                            ),
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
}
