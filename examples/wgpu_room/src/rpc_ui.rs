use crate::service::{AsyncCmd, LkService};
use egui::Color32;
use livekit::prelude::*;
use parking_lot::Mutex;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_INVOCATIONS: usize = 200;
const PAYLOAD_PREVIEW_CHARS: usize = 256;
const RESPONSE_PREVIEW_CHARS: usize = 40;

static NEXT_SEND_ID: AtomicU64 = AtomicU64::new(1);

pub struct RpcUiState {
    send_destination: Option<ParticipantIdentity>,
    send_method: String,
    send_payload: String,
    send_in_flight: Option<u64>,
    send_result: Option<SendResult>,

    register_method: String,
    register_error: Option<String>,

    handlers: BTreeMap<String, Arc<Mutex<HandlerEntry>>>,
}

struct HandlerEntry {
    method: String,
    reply: String,
    invocations: VecDeque<Invocation>,
    invocation_count: u64,
}

struct Invocation {
    n: u64,
    caller: String,
    payload_len: usize,
    received_at: SystemTime,
    payload_preview: String,
}

enum SendResult {
    Ok(String),
    Err { code: u32, message: String },
}

impl Default for RpcUiState {
    fn default() -> Self {
        Self {
            send_destination: None,
            send_method: String::new(),
            send_payload: String::new(),
            send_in_flight: None,
            send_result: None,
            register_method: String::new(),
            register_error: None,
            handlers: BTreeMap::new(),
        }
    }
}

impl RpcUiState {
    pub fn handle_send_result(&mut self, request_id: u64, result: Result<String, RpcError>) {
        if self.send_in_flight == Some(request_id) {
            self.send_in_flight = None;
        }
        self.send_result = Some(match result {
            Ok(s) => SendResult::Ok(s),
            Err(e) => SendResult::Err { code: e.code, message: e.message },
        });
    }

    pub fn on_disconnect(&mut self) {
        self.handlers.clear();
        self.send_in_flight = None;
        self.send_destination = None;
        self.register_error = None;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, service: &LkService, room: &Arc<Room>) {
        self.show_send(ui, service, room);
        ui.add_space(8.0);
        ui.separator();
        self.show_register(ui, service, room);
        self.show_handler_cards(ui, service, room);
    }

    fn show_send(&mut self, ui: &mut egui::Ui, service: &LkService, room: &Arc<Room>) {
        ui.label(egui::RichText::new("Send RPC").strong());

        let participants = room.remote_participants();
        let mut idents: Vec<ParticipantIdentity> = participants.keys().cloned().collect();
        idents.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        if let Some(sel) = self.send_destination.as_ref() {
            if !participants.contains_key(sel) {
                self.send_destination = None;
            }
        }

        ui.horizontal(|ui| {
            ui.label("To:");
            let combo_label =
                self.send_destination.as_ref().map(|i| i.as_str().to_string()).unwrap_or_else(
                    || {
                        if idents.is_empty() {
                            "(no remote participants)".to_string()
                        } else {
                            "(select)".to_string()
                        }
                    },
                );
            egui::ComboBox::from_id_salt("rpc_dest_combo").selected_text(combo_label).show_ui(
                ui,
                |ui| {
                    for ident in &idents {
                        ui.selectable_value(
                            &mut self.send_destination,
                            Some(ident.clone()),
                            ident.as_str(),
                        );
                    }
                },
            );
        });

        ui.horizontal(|ui| {
            ui.label("Method:");
            ui.add(egui::TextEdit::singleline(&mut self.send_method).desired_width(f32::INFINITY));
        });

        ui.horizontal(|ui| {
            ui.label("Payload:");
            if ui.small_button("Hello").clicked() {
                self.send_payload = "Hello world".to_string();
            }
            if ui.small_button("20k").clicked() {
                self.send_payload = "X".repeat(20_000);
            }
        });
        let max_h = ui.text_style_height(&egui::TextStyle::Body) * 5.0 + 8.0;
        egui::ScrollArea::vertical().id_salt("rpc_send_payload_scroll").max_height(max_h).show(
            ui,
            |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.send_payload)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
            },
        );

        let can_send = self.send_in_flight.is_none()
            && self.send_destination.is_some()
            && !self.send_method.trim().is_empty();

        ui.horizontal(|ui| {
            ui.add_enabled_ui(can_send, |ui| {
                if ui.button("Send").clicked() {
                    let dest = self.send_destination.as_ref().unwrap().0.clone();
                    let method = self.send_method.clone();
                    let payload = self.send_payload.clone();
                    let request_id = NEXT_SEND_ID.fetch_add(1, Ordering::Relaxed);
                    self.send_in_flight = Some(request_id);
                    self.send_result = None;
                    let _ = service.send(AsyncCmd::RpcSendRequest {
                        destination: dest,
                        method,
                        payload,
                        request_id,
                    });
                }
            });
            if self.send_in_flight.is_some() {
                ui.spinner();
            }
        });

        if self.send_in_flight.is_some() {
            let dest =
                self.send_destination.as_ref().map(|i| i.as_str().to_string()).unwrap_or_default();
            ui.colored_label(Color32::GRAY, format!("Sending to {} {}...", dest, self.send_method));
        } else {
            match &self.send_result {
                Some(SendResult::Ok(s)) => {
                    ui.colored_label(Color32::LIGHT_GREEN, format!("OK: {}", preview_response(s)));
                }
                Some(SendResult::Err { code, message }) => {
                    ui.colored_label(
                        Color32::LIGHT_RED,
                        format!("Error {}: {}", code, message),
                    );
                }
                None => {}
            }
        }
    }

    fn show_register(&mut self, ui: &mut egui::Ui, service: &LkService, room: &Arc<Room>) {
        ui.label(egui::RichText::new("Handlers").strong());

        let mut do_register = false;
        ui.horizontal(|ui| {
            ui.label("Topic:");
            let resp =
                ui.add(egui::TextEdit::singleline(&mut self.register_method).desired_width(120.0));
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                do_register = true;
            }
            if ui.button("Register Handler").clicked() {
                do_register = true;
            }
        });

        if do_register {
            self.register_error = None;
            let method = self.register_method.trim().to_string();
            if method.is_empty() {
                self.register_error = Some("Topic is empty".to_string());
            } else if self.handlers.contains_key(&method) {
                self.register_error = Some(format!("Handler already registered for '{}'", method));
            } else {
                let entry = Arc::new(Mutex::new(HandlerEntry {
                    method: method.clone(),
                    reply: String::new(),
                    invocations: VecDeque::new(),
                    invocation_count: 0,
                }));
                let entry_for_cb = entry.clone();
                let _guard = service.runtime().enter();
                room.local_participant().register_rpc_method(method.clone(), move |data| {
                    let entry_for_cb = entry_for_cb.clone();
                    Box::pin(async move {
                        let reply = {
                            let mut g = entry_for_cb.lock();
                            push_invocation(&mut g, &data);
                            g.reply.clone()
                        };
                        Ok(reply)
                    })
                });
                self.handlers.insert(method, entry);
                self.register_method.clear();
            }
        }

        if let Some(err) = &self.register_error {
            ui.colored_label(Color32::LIGHT_RED, err);
        }
    }

    fn show_handler_cards(&mut self, ui: &mut egui::Ui, service: &LkService, room: &Arc<Room>) {
        let methods: Vec<String> = self.handlers.keys().cloned().collect();
        let mut to_remove: Option<String> = None;

        for method in methods {
            let entry = self.handlers.get(&method).unwrap().clone();
            ui.add_space(6.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                let mut guard = entry.lock();
                ui.horizontal(|ui| {
                    ui.monospace(egui::RichText::new(&guard.method).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Unregister").clicked() {
                            to_remove = Some(guard.method.clone());
                        }
                    });
                });

                ui.horizontal(|ui| {
                    ui.label("Reply:");
                    if ui.small_button("Hello").clicked() {
                        guard.reply = "Hello world".to_string();
                    }
                    if ui.small_button("20k").clicked() {
                        guard.reply = "X".repeat(20_000);
                    }
                });
                let max_h = ui.text_style_height(&egui::TextStyle::Body) * 5.0 + 8.0;
                egui::ScrollArea::vertical()
                    .id_salt(format!("rpc_handler_reply_scroll_{}", guard.method))
                    .max_height(max_h)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut guard.reply)
                                .desired_rows(1)
                                .desired_width(f32::INFINITY),
                        );
                    });

                ui.label(format!("Invocations ({})", guard.invocation_count));

                if guard.invocations.is_empty() {
                    ui.colored_label(Color32::GRAY, "No invocations yet");
                } else {
                    for inv in guard.invocations.iter() {
                        let meta = format!(
                            "#{} | {} | {} | {}",
                            inv.n,
                            inv.caller,
                            format_size(inv.payload_len),
                            format_ts(inv.received_at),
                        );
                        ui.add(egui::Label::new(
                            egui::RichText::new(meta).small().color(Color32::GRAY),
                        ));
                        ui.add(egui::Label::new(
                            egui::RichText::new(&inv.payload_preview).monospace(),
                        ));
                        ui.separator();
                    }
                }
            });
        }

        if let Some(m) = to_remove {
            let _guard = service.runtime().enter();
            room.local_participant().unregister_rpc_method(m.clone());
            self.handlers.remove(&m);
        }
    }
}

fn push_invocation(entry: &mut HandlerEntry, data: &RpcInvocationData) {
    entry.invocation_count += 1;
    let payload_len = data.payload.as_bytes().len();
    let payload_preview = truncate_chars(&data.payload, PAYLOAD_PREVIEW_CHARS);
    entry.invocations.push_back(Invocation {
        n: entry.invocation_count,
        caller: data.caller_identity.as_str().to_string(),
        payload_len,
        received_at: SystemTime::now(),
        payload_preview,
    });
    while entry.invocations.len() > MAX_INVOCATIONS {
        entry.invocations.pop_front();
    }
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut iter = s.chars();
    let head: String = iter.by_ref().take(max_chars).collect();
    if iter.next().is_some() {
        format!("{}...", head)
    } else {
        head
    }
}

fn preview_response(s: &str) -> String {
    let bytes = s.as_bytes().len();
    let mut iter = s.chars();
    let head: String = iter.by_ref().take(RESPONSE_PREVIEW_CHARS).collect();
    if iter.next().is_some() {
        format!("{}... ({}B)", head, bytes)
    } else {
        head
    }
}

fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else {
        format!("{:.2}KB", bytes as f64 / 1024.0)
    }
}

fn format_ts(ts: SystemTime) -> String {
    let d = ts.duration_since(UNIX_EPOCH).unwrap_or_default();
    let total = d.as_secs();
    let h = (total / 3600) % 24;
    let m = (total / 60) % 60;
    let s = total % 60;
    let ms = d.subsec_millis();
    format!("{:02}:{:02}:{:02}.{:03}Z", h, m, s, ms)
}
