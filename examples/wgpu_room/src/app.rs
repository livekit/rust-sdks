use crate::{
    data_track::{LocalDataTrackTile, RemoteDataTrackTile, MAX_VALUE, TIME_WINDOW},
    service::{AsyncCmd, LkService, UiCmd},
    video_grid::VideoGrid,
    video_renderer::VideoRenderer,
};
use egui::{emath, epaint, pos2, Color32, CornerRadius, Rect, Stroke};
use livekit::{e2ee::EncryptionType, prelude::*, track::VideoQuality, SimulateScenario};
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
    local_data_tracks: Vec<LocalDataTrackTile>,
    remote_data_tracks: Vec<RemoteDataTrackTile>,
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
            local_data_tracks: Vec::new(),
            remote_data_tracks: Vec::new(),
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
            UiCmd::DataTrackPublished { track } => {
                self.local_data_tracks.push(LocalDataTrackTile::new(track));
            }
            UiCmd::DataTrackUnpublished => {
                self.local_data_tracks.clear();
            }
            UiCmd::RoomEvent { event } => {
                log::info!("{:?}", event);
                match event {
                    RoomEvent::TrackSubscribed { track, publication: _, participant } => {
                        if let RemoteTrack::Video(ref video_track) = track {
                            let video_renderer = VideoRenderer::new(
                                self.async_runtime.handle(),
                                self.render_state.clone(),
                                video_track.rtc_track(),
                            );
                            self.video_renderers
                                .insert((participant.identity(), track.sid()), video_renderer);
                        }
                    }
                    RoomEvent::TrackUnsubscribed { track, publication: _, participant } => {
                        self.video_renderers.remove(&(participant.identity(), track.sid()));
                    }
                    RoomEvent::LocalTrackPublished { track, publication: _, participant } => {
                        if let LocalTrack::Video(ref video_track) = track {
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
                    RoomEvent::RemoteDataTrackPublished(track) => {
                        self.remote_data_tracks.push(RemoteDataTrackTile::new(
                            self.async_runtime.handle(),
                            track,
                        ));
                    }
                    RoomEvent::Disconnected { reason: _ } => {
                        self.video_renderers.clear();
                        self.local_data_tracks.clear();
                        self.remote_data_tracks.clear();
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
                if ui.button("DataTrack").clicked() {
                    let _ = self.service.send(AsyncCmd::ToggleDataTrack);
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
                        ui.label("Simulcasted - ");
                        let is_simulcasted = publication.simulcasted();
                        ui.label(if is_simulcasted { "Yes" } else { "No" });
                        if is_simulcasted {
                            ui.menu_button("Set Quality", |ui| {
                                let publication = publication.clone();
                                if ui.button("Low").clicked() {
                                    let _ = self.service.send(AsyncCmd::SetVideoQuality {
                                        publication,
                                        quality: VideoQuality::Low,
                                    });
                                } else if ui.button("Medium").clicked() {
                                    let _ = self.service.send(AsyncCmd::SetVideoQuality {
                                        publication,
                                        quality: VideoQuality::Medium,
                                    });
                                } else if ui.button("High").clicked() {
                                    let _ = self.service.send(AsyncCmd::SetVideoQuality {
                                        publication,
                                        quality: VideoQuality::High,
                                    });
                                }
                            });
                        }
                    });

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

    /// Draw a grid of all track tiles (video + data)
    fn central_panel(&mut self, ui: &mut egui::Ui) {
        let room = self.service.room();
        let connected = room.is_some();

        let has_tiles = !self.video_renderers.is_empty()
            || !self.local_data_tracks.is_empty()
            || !self.remote_data_tracks.is_empty();

        if connected && !has_tiles {
            ui.centered_and_justified(|ui| {
                ui.label("No tracks subscribed");
            });
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            VideoGrid::new("default_grid").max_columns(6).show(ui, |ui| {
                if connected {
                    let room = room.as_ref().unwrap();

                    for ((participant_id, _), video_renderer) in &self.video_renderers {
                        ui.video_frame(|ui| {
                            if let Some(p) = room.remote_participants().get(participant_id) {
                                draw_video(p.name().as_str(), p.is_speaking(), video_renderer, ui);
                            } else {
                                let lp = room.local_participant();
                                draw_video(lp.name().as_str(), lp.is_speaking(), video_renderer, ui);
                            }
                        });
                    }

                    for tile in &mut self.local_data_tracks {
                        ui.video_frame(|ui| draw_local_data_track(tile, ui));
                    }

                    for tile in &self.remote_data_tracks {
                        ui.video_frame(|ui| draw_remote_data_track(tile, ui));
                    }
                } else {
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

fn draw_local_data_track(tile: &mut LocalDataTrackTile, ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(rect, CornerRadius::default(), ui.style().visuals.code_bg_color);

    let padding = 16.0;
    let item_spacing = ui.spacing().item_spacing.x;
    let text_edit_width = ui.spacing().interact_size.x;
    let rail_width = (rect.width() - padding * 2.0 - item_spacing - text_edit_width).max(20.0);

    ui.add_space(((rect.height() - 20.0) / 2.0).max(0.0));
    ui.horizontal(|ui| {
        ui.add_space(padding);
        ui.spacing_mut().slider_width = rail_width;
        let slider = egui::Slider::new(&mut tile.slider_value, 0..=512);
        if ui.add(slider).changed() {
            tile.push_value();
        }
    });

    ui.painter().text(
        egui::pos2(rect.min.x + 5.0, rect.max.y - 5.0),
        egui::Align2::LEFT_BOTTOM,
        format!("Data: {} (local)", tile.name),
        egui::FontId::default(),
        egui::Color32::WHITE,
    );
}

fn draw_remote_data_track(tile: &RemoteDataTrackTile, ui: &mut egui::Ui) {
    let rect = ui.available_rect_before_wrap();
    let painter = ui.painter();

    let bg = Color32::from_rgb(0x1a, 0x1a, 0x2e);
    painter.rect_filled(rect, CornerRadius::default(), bg);

    let v_margin = rect.height() * 0.25;
    let h_margin = 8.0;
    let label_width = 32.0;
    let plot_rect = Rect::from_min_max(
        pos2(rect.min.x + h_margin + label_width, rect.min.y + v_margin),
        pos2(rect.max.x - h_margin, rect.max.y - v_margin),
    );

    let time_window_secs = TIME_WINDOW.as_secs_f32();
    let to_screen = emath::RectTransform::from_to(
        Rect::from_x_y_ranges(0.0..=time_window_secs, MAX_VALUE..=0.0),
        plot_rect,
    );

    let guide_color = Color32::from_rgb(0x40, 0x40, 0x50);
    let max_y = (to_screen * pos2(0.0, MAX_VALUE)).y;
    let min_y = (to_screen * pos2(0.0, 0.0)).y;
    painter.line_segment(
        [pos2(plot_rect.min.x, max_y), pos2(plot_rect.max.x, max_y)],
        Stroke::new(1.0, guide_color),
    );
    painter.line_segment(
        [pos2(plot_rect.min.x, min_y), pos2(plot_rect.max.x, min_y)],
        Stroke::new(1.0, guide_color),
    );

    let now = std::time::Instant::now();
    let mut points = tile.points.lock();
    while points.back().is_some_and(|(t, _)| now.duration_since(*t) > TIME_WINDOW) {
        points.pop_back();
    }

    if points.is_empty() {
        drop(points);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Waiting...",
            egui::FontId::proportional(24.0),
            Color32::WHITE,
        );
    } else {
        let line_color = Color32::from_rgb(0xFF, 0x44, 0x44);
        let mut screen_points = Vec::with_capacity(points.len() + 1);

        let (_, newest_val) = points[0];
        let newest_screen = to_screen * pos2(0.0, newest_val as f32);
        screen_points.push(newest_screen);

        for &(t, val) in points.iter() {
            let age = now.duration_since(t).as_secs_f32();
            screen_points.push(to_screen * pos2(age, val as f32));
        }

        drop(points);
        painter.add(epaint::Shape::line(
            screen_points,
            epaint::PathStroke::new(2.0, line_color),
        ));

        painter.circle_filled(newest_screen, 4.0, line_color);

        painter.text(
            pos2(plot_rect.min.x - 4.0, newest_screen.y),
            egui::Align2::RIGHT_CENTER,
            newest_val.to_string(),
            egui::FontId::proportional(14.0),
            Color32::WHITE,
        );
    }

    painter.text(
        pos2(rect.min.x + 5.0, rect.max.y - 5.0),
        egui::Align2::LEFT_BOTTOM,
        format!("Data: {} ({})", tile.name, tile.publisher_identity),
        egui::FontId::default(),
        Color32::WHITE,
    );
}
