use crate::{
    service::{AsyncCmd, LkService, UiCmd},
    video_grid::VideoGrid,
    video_renderer::VideoRenderer,
};
use egui::{CornerRadius, Stroke};
use livekit::{e2ee::EncryptionType, prelude::*, SimulateScenario};
use std::collections::HashMap;

/// The state of the application are saved on app exit and restored on app start.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct AppState {
    url: String,
    token: String,
    key: String,
    auto_subscribe: bool,
    enable_e2ee: bool,
}

pub struct LkApp {
    async_runtime: tokio::runtime::Runtime,
    state: AppState,
    video_renderers: HashMap<(ParticipantIdentity, TrackSid), VideoRenderer>,
    connecting: bool,
    connection_failure: Option<String>,
    render_state: egui_wgpu::RenderState,
    service: LkService,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            url: "ws://localhost:7880".to_string(),
            token: "".to_string(),
            auto_subscribe: true,
            enable_e2ee: false,
            key: "".to_string(),
        }
    }
}

impl LkApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
            .unwrap_or_default();

        let async_runtime =
            tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();

        Self {
            service: LkService::new(async_runtime.handle()),
            async_runtime,
            state,
            video_renderers: HashMap::new(),
            connecting: false,
            connection_failure: None,
            render_state: cc.wgpu_render_state.clone().unwrap(),
        }
    }

    fn event(&mut self, event: UiCmd) {
        match event {
            UiCmd::ConnectResult { result } => {
                self.connecting = false;
                if let Err(err) = result {
                    self.connection_failure = Some(err.to_string());
                }
            }
            UiCmd::RoomEvent { event } => {
                log::info!("{:?}", event);
                match event {
                    RoomEvent::TrackSubscribed { track, publication: _, participant } => {
                        if let RemoteTrack::Video(ref video_track) = track {
                            // Create a new VideoRenderer
                            let video_renderer = VideoRenderer::new(
                                self.async_runtime.handle(),
                                self.render_state.clone(),
                                video_track.rtc_track(),
                            );
                            self.video_renderers
                                .insert((participant.identity(), track.sid()), video_renderer);
                        } else if let RemoteTrack::Audio(_) = track {
                            // TODO(theomonnom): Once we support media devices, we can play audio tracks here
                        }
                    }
                    RoomEvent::TrackUnsubscribed { track, publication: _, participant } => {
                        self.video_renderers.remove(&(participant.identity(), track.sid()));
                    }
                    RoomEvent::LocalTrackPublished { track, publication: _, participant } => {
                        if let LocalTrack::Video(ref video_track) = track {
                            // Also create a new VideoRenderer for local tracks
                            let video_renderer = VideoRenderer::new(
                                self.async_runtime.handle(),
                                self.render_state.clone(),
                                video_track.rtc_track(),
                            );
                            self.video_renderers
                                .insert((participant.identity(), track.sid()), video_renderer);
                        }
                    }
                    RoomEvent::LocalTrackUnpublished { publication, participant } => {
                        self.video_renderers.remove(&(participant.identity(), publication.sid()));
                    }
                    RoomEvent::Disconnected { reason: _ } => {
                        self.video_renderers.clear();
                    }
                    _ => {}
                }
            }
        }
    }

    fn top_panel(&mut self, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("Simulate", |ui| {
                let scenarios = [
                    SimulateScenario::SignalReconnect,
                    SimulateScenario::Speaker,
                    SimulateScenario::NodeFailure,
                    SimulateScenario::ServerLeave,
                    SimulateScenario::Migration,
                    SimulateScenario::ForceTcp,
                    SimulateScenario::ForceTls,
                ];

                for scenario in scenarios {
                    if ui.button(format!("{:?}", scenario)).clicked() {
                        let _ = self.service.send(AsyncCmd::SimulateScenario { scenario });
                    }
                }
            });

            ui.menu_button("Publish", |ui| {
                if ui.button("Logo").clicked() {
                    let _ = self.service.send(AsyncCmd::ToggleLogo);
                }
                if ui.button("SineWave").clicked() {
                    let _ = self.service.send(AsyncCmd::ToggleSine);
                }
            });

            ui.menu_button("Debug", |ui| {
                if ui.button("Log stats").clicked() {
                    let _ = self.service.send(AsyncCmd::LogStats);
                }
            });
        });
    }

    /// Connection form and room info
    fn left_panel(&mut self, ui: &mut egui::Ui) {
        let room = self.service.room();
        let connected = room.is_some()
            && room.as_ref().unwrap().connection_state() == ConnectionState::Connected;

        ui.add_space(8.0);
        ui.monospace("Livekit - Connect to a room");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Url: ");
            ui.text_edit_singleline(&mut self.state.url);
        });

        ui.horizontal(|ui| {
            ui.label("Token: ");
            ui.text_edit_singleline(&mut self.state.token);
        });

        ui.horizontal(|ui| {
            ui.label("E2ee Key: ");
            ui.text_edit_singleline(&mut self.state.key);
        });

        ui.horizontal(|ui| {
            ui.add_enabled_ui(true, |ui| {
                ui.checkbox(&mut self.state.enable_e2ee, "Enable E2ee");
            });
        });

        ui.horizontal(|ui| {
            ui.add_enabled_ui(!connected && !self.connecting, |ui| {
                if ui.button("Connect").clicked() {
                    self.connecting = true;
                    self.connection_failure = None;
                    let _ = self.service.send(AsyncCmd::RoomConnect {
                        url: self.state.url.clone(),
                        token: self.state.token.clone(),
                        auto_subscribe: self.state.auto_subscribe,
                        enable_e2ee: self.state.enable_e2ee,
                        key: self.state.key.clone(),
                    });
                }
            });

            if self.connecting {
                ui.spinner();
            } else if connected && ui.button("Disconnect").clicked() {
                let _ = self.service.send(AsyncCmd::RoomDisconnect);
            }
        });

        if ui.button("E2eeKeyRatchet").clicked() {
            let _ = self.service.send(AsyncCmd::E2eeKeyRatchet);
        }

        ui.horizontal(|ui| {
            ui.add_enabled_ui(true, |ui| {
                ui.checkbox(&mut self.state.auto_subscribe, "Auto Subscribe");
            });
        });

        if let Some(err) = &self.connection_failure {
            ui.colored_label(egui::Color32::RED, err);
        }

        if let Some(room) = room.as_ref() {
            ui.label(format!("Name: {}", room.name()));
            //ui.label(format!("Sid: {}", String::from(room.sid().await)));
            ui.label(format!("ConnectionState: {:?}", room.connection_state()));
            ui.label(format!("ParticipantCount: {:?}", room.remote_participants().len() + 1));
        }

        egui::warn_if_debug_build(ui);
        ui.separator();
    }

    /// Show remote_participants and their tracks
    fn right_panel(&self, ui: &mut egui::Ui) {
        ui.label("Participants");
        ui.separator();

        let Some(room) = self.service.room() else {
            return;
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Iterate with sorted keys to avoid flickers (Because this is a immediate mode UI)
            let participants = room.remote_participants();
            let mut sorted_participants =
                participants.keys().cloned().collect::<Vec<ParticipantIdentity>>();
            sorted_participants.sort_by(|a, b| a.as_str().cmp(b.as_str()));

            for psid in sorted_participants {
                let participant = participants.get(&psid).unwrap();
                let tracks = participant.track_publications();
                let mut sorted_tracks = tracks.keys().cloned().collect::<Vec<TrackSid>>();
                sorted_tracks.sort_by(|a, b| a.as_str().cmp(b.as_str()));

                ui.monospace(&participant.identity().0);
                for tsid in sorted_tracks {
                    let publication = tracks.get(&tsid).unwrap().clone();

                    ui.horizontal(|ui| {
                        ui.label("Encrypted - ");
                        let enc_type = publication.encryption_type();
                        if enc_type == EncryptionType::None {
                            ui.colored_label(egui::Color32::RED, format!("{:?}", enc_type));
                        } else {
                            ui.colored_label(egui::Color32::GREEN, format!("{:?}", enc_type));
                        }
                    });

                    ui.label(format!("{} - {:?}", publication.name(), publication.source()));

                    ui.horizontal(|ui| {
                        if publication.is_muted() {
                            ui.colored_label(egui::Color32::DARK_GRAY, "Muted");
                        }

                        if publication.is_subscribed() {
                            ui.colored_label(egui::Color32::GREEN, "Subscribed");
                        } else {
                            ui.colored_label(egui::Color32::RED, "Unsubscribed");
                        }

                        if publication.is_subscribed() {
                            if ui.button("Unsubscribe").clicked() {
                                let _ =
                                    self.service.send(AsyncCmd::UnsubscribeTrack { publication });
                            }
                        } else if ui.button("Subscribe").clicked() {
                            let _ = self.service.send(AsyncCmd::SubscribeTrack { publication });
                        }
                    });
                }
                ui.separator();
            }
        });
    }

    /// Draw a video grid of all participants
    fn central_panel(&mut self, ui: &mut egui::Ui) {
        let room = self.service.room();
        let show_videos = self.service.room().is_some();

        if show_videos && self.video_renderers.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label("No video tracks subscribed");
            });
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            VideoGrid::new("default_grid").max_columns(6).show(ui, |ui| {
                if show_videos {
                    // Draw participant videos
                    for ((participant_sid, _), video_renderer) in &self.video_renderers {
                        ui.video_frame(|ui| {
                            let room = room.as_ref().unwrap().clone();

                            if let Some(participant) =
                                room.remote_participants().get(participant_sid)
                            {
                                draw_video(
                                    participant.name().as_str(),
                                    participant.is_speaking(),
                                    video_renderer,
                                    ui,
                                );
                            } else {
                                draw_video(
                                    room.local_participant().name().as_str(),
                                    room.local_participant().is_speaking(),
                                    video_renderer,
                                    ui,
                                );
                            }
                        });
                    }
                } else {
                    // Draw video skeletons when we're not connected
                    for _ in 0..5 {
                        ui.video_frame(|ui| {
                            egui::Frame::none().fill(ui.style().visuals.code_bg_color).show(
                                ui,
                                |ui| {
                                    ui.allocate_space(ui.available_size());
                                },
                            );
                        });
                    }
                }
            })
        });
    }
}

impl eframe::App for LkApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.state);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(event) = self.service.try_recv() {
            self.event(event);
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            self.top_panel(ui);
        });

        egui::SidePanel::left("left_panel").resizable(true).width_range(20.0..=360.0).show(
            ctx,
            |ui| {
                self.left_panel(ui);
            },
        );

        /*egui::TopBottomPanel::bottom("bottom_panel")
        .resizable(true)
        .height_range(20.0..=256.0)
        .show(ctx, |ui| {
            self.bottom_panel(ui);
        });*/

        egui::SidePanel::right("right_panel").resizable(true).width_range(20.0..=360.0).show(
            ctx,
            |ui| {
                self.right_panel(ui);
            },
        );

        egui::CentralPanel::default().show(ctx, |ui| {
            self.central_panel(ui);
        });

        ctx.request_repaint();
    }
}

/// Draw a wgpu texture to the VideoGrid
fn draw_video(name: &str, speaking: bool, video_renderer: &VideoRenderer, ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    let inner_rect = rect.shrink(1.0);

    if speaking {
        ui.painter().rect(
            rect,
            CornerRadius::default(),
            egui::Color32::GREEN,
            Stroke::NONE,
            egui::StrokeKind::Inside,
        );
    }

    // Always draw a background in case we still didn't receive a frame
    let resolution = video_renderer.resolution();
    if let Some(tex) = video_renderer.texture_id() {
        ui.painter().image(
            tex,
            inner_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }

    ui.painter().text(
        egui::pos2(rect.min.x + 5.0, rect.max.y - 5.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{}x{} {}", resolution.0, resolution.1, name),
        egui::FontId::default(),
        egui::Color32::WHITE,
    );
}
