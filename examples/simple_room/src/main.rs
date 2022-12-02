use std::convert::TryInto;
use std::ops::DerefMut;
use std::{num::NonZeroU32, time::Duration};

use egui_wgpu::WgpuConfiguration;
use livekit::webrtc::media_stream::VideoTrack;
use livekit::webrtc::video_frame_buffer::{
    PlanarYuv8Buffer, PlanarYuvBuffer, VideoFrameBufferTrait,
};
use livekit::webrtc::yuv_helper;
use std::sync::{Arc, Mutex};
use video_renderer::VideoRenderer;
use wgpu::{Device, Queue};

use tokio::time::sleep;

use livekit::room::track::remote_track::RemoteTrackHandle;
use livekit::room::{Room, RoomError};

const URL: &str = "ws://localhost:7880";
const TOKEN : &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjIzODQ4MDY0NzMsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJuYXRpdmUiLCJuYmYiOjE2NjQ4MDY0NzMsInN1YiI6Im5hdGl2ZSIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.BgVdBnq3XFD3_BQHoe1azqjifYysubgFl6Qlzu9IQGI";

// eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjIzODQ4MDY3MzAsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJ3ZWIiLCJuYmYiOjE2NjQ4MDY3MzAsInN1YiI6IndlYiIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.VbDoULjX1CVGZu2sPy3SvWYlVZUBXxQVPmdB9BnmlN4

mod video_renderer;

use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder, WindowId},
};

struct AppState {
    room: Room,
    demo: egui_demo_lib::DemoWindows,
    egui_context: egui::Context,
    egui_state: egui_winit::State,
    egui_painter: egui_wgpu::winit::Painter,
    window: winit::window::Window,
}

impl AppState {
    fn on_event<T>(&mut self, event: Event<'_, T>, control_flow: &mut ControlFlow) {
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

    fn render(&mut self) {
        let raw_inputs = self.egui_state.take_egui_input(&self.window);
        let full_output = self.egui_context.run(raw_inputs, |ctx| {
            //self.ui(ctx);
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

struct App {
    rt: tokio::runtime::Runtime,
}

impl App {
    pub fn new(rt: tokio::runtime::Runtime) -> Self {
        Self { rt }
    }

    pub fn run(&mut self) {
        self.rt.block_on(async {
            let event_loop = EventLoop::new();
            let window = WindowBuilder::new().build(&event_loop).unwrap();

            let egui_context = egui::Context::default();
            let egui_state = egui_winit::State::new(&event_loop);
            let mut egui_painter =
                egui_wgpu::winit::Painter::new(WgpuConfiguration::default(), 1, 32);
            unsafe {
                egui_painter.set_window(Some(&window));
            }

            let mut inner = AppState {
                room: Room::new(),
                demo: egui_demo_lib::DemoWindows::default(),
                egui_context,
                egui_state,
                egui_painter,
                window,
            };

            inner
                .room
                .events()
                .on_participant_connected(|_event| async move {});

            inner.room.events().on_track_subscribed({
                let test = Arc::new(Mutex::new(None));

                let egui_render = inner.egui_painter.render_state().clone().unwrap();

                move |event| {
                    let test = test.clone();
                    let egui_render = egui_render.clone();

                    async move {
                        let track = event.publication.track().unwrap();
                        if let RemoteTrackHandle::Video(video_track) = track {
                            *test.lock().unwrap() =
                                Some(VideoRenderer::new(egui_render, video_track.rtc_track()))
                        }
                    }
                }
            });

            inner.room.connect(URL, TOKEN).await.unwrap();

            tokio::spawn(async {
                loop {
                    println!("Test");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            });

            tokio::task::block_in_place(move || loop {
                event_loop.run(move |event, _, control_flow| {
                    inner.on_event(event, control_flow);
                });
            });
        });
    }
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut app = App::new(rt);
    app.run();
}
