use crate::events::DemoEvent;
use crate::video_grid::VideoGrid;
use crate::video_renderer::VideoRenderer;
use egui_wgpu::WgpuConfiguration;
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::mpsc;

use livekit::room::Room;

const URL: &str = "ws://localhost:7880";
const TOKEN : &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjIzODQ4MDY0NzMsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJuYXRpdmUiLCJuYmYiOjE2NjQ4MDY0NzMsInN1YiI6Im5hdGl2ZSIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.BgVdBnq3XFD3_BQHoe1azqjifYysubgFl6Qlzu9IQGI";

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

    renderers: Vec<VideoRenderer>,
    egui_context: egui::Context,
    egui_state: egui_winit::State,
    egui_painter: egui_wgpu::winit::Painter,
    window: winit::window::Window,
    event_tx: mpsc::UnboundedSender<DemoEvent>,

    // UI State
    lk_url: String,
    lk_token: String,
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

        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<DemoEvent>();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<DemoEvent>();

        let state = Arc::new(AppState {
            room: Mutex::new(Room::new()),
            connecting: AtomicBool::new(false),
        });

        let mut app = App {
            state: state.clone(),
            renderers: Vec::default(),
            egui_context,
            egui_state,
            egui_painter,
            window,
            event_tx,
            lk_url: "ws://localhost:8080/".to_owned(),
            lk_token: "your token".to_owned(),
        };

        // Async event loop
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                match event {
                    DemoEvent::RoomConnect { url, token } => {
                        state.connecting.store(true, Ordering::SeqCst);

                        let mut room = state.room.lock();
                        room.connect(&url, &token).await.unwrap();

                        state.connecting.store(false, Ordering::SeqCst);
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
                });
                ui.menu_button("Simulate", |ui| {});
            });
        });

        egui::SidePanel::right("room_panel")
            .default_width(128.0)
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
                        self.event_tx
                            .send(DemoEvent::RoomConnect {
                                url: self.lk_url.clone(),
                                token: self.lk_token.clone(),
                            })
                            .unwrap();
                    }

                    if connecting {
                        ui.spinner();
                    }
                });

                ui.allocate_space(ui.available_size());
            });

        egui::CentralPanel::default().show(ui.ctx(), |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                VideoGrid::new("default_grid")
                    .max_columns(6)
                    .show(ui, |ui| {
                        for _ in 0..20 {
                            ui.video_frame(|ui| {
                                egui::Frame::none()
                                    .fill(egui::Color32::DARK_GRAY)
                                    .show(ui, |ui| {
                                        ui.allocate_space(ui.available_size());
                                    });
                            });
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
