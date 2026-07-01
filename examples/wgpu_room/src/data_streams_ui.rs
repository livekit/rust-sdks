use crate::service::LkService;
use egui::Color32;
use livekit::prelude::*;
use livekit::{
    ByteStreamReader, StreamByteOptions, StreamReader, StreamTextOptions, TakeCell,
    TextStreamReader,
};
use parking_lot::Mutex;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RECEIVED: usize = 100;
const PREVIEW_CHARS: usize = 256;
const PREVIEW_BYTES: usize = 64;

/// Whether a stream carries text or raw bytes. Used for both sending and subscribing.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StreamKind {
    Text,
    Bytes,
}

impl StreamKind {
    fn label(self) -> &'static str {
        match self {
            StreamKind::Text => "text",
            StreamKind::Bytes => "bytes",
        }
    }
}

/// Outcome of the most recent send, shared with the spawned send task.
#[derive(Clone)]
enum SendState {
    Idle,
    Sending,
    Ok(String),
    Err(String),
}

/// A single received stream, rendered as a card.
struct ReceivedStream {
    n: u64,
    sender: String,
    received_at: SystemTime,
    size: usize,
    compressed: bool,
    inline: bool,
    preview: String,
}

/// Accumulates streams received for one (topic, kind) subscription.
struct TopicEntry {
    topic: String,
    kind: StreamKind,
    received: VecDeque<ReceivedStream>,
    count: u64,
}

pub struct DataStreamsUiState {
    // Send section
    send_kind: StreamKind,
    send_topic: String,
    send_destination: Option<ParticipantIdentity>,
    send_content: String,
    send_hex: bool,
    send_state: Arc<Mutex<SendState>>,

    // Subscribe section
    sub_topic: String,
    sub_kind: StreamKind,
    sub_error: Option<String>,
    subscriptions: BTreeMap<(String, StreamKind), Arc<Mutex<TopicEntry>>>,
}

impl Default for DataStreamsUiState {
    fn default() -> Self {
        Self {
            send_kind: StreamKind::Text,
            send_topic: String::new(),
            send_destination: None,
            send_content: String::new(),
            send_hex: false,
            send_state: Arc::new(Mutex::new(SendState::Idle)),
            sub_topic: String::new(),
            sub_kind: StreamKind::Text,
            sub_error: None,
            subscriptions: BTreeMap::new(),
        }
    }
}

impl DataStreamsUiState {
    pub fn on_disconnect(&mut self) {
        // Subscriptions are pure UI filters (no room registration), so we keep them but clear
        // the streams received during the previous session.
        for entry in self.subscriptions.values() {
            let mut g = entry.lock();
            g.received.clear();
            g.count = 0;
        }
        *self.send_state.lock() = SendState::Idle;
        self.send_destination = None;
    }

    /// Routes an incoming text stream to a matching subscription (if any), reading it in the
    /// background. Unmatched streams are dropped (the reader is never taken).
    pub fn on_text_stream(
        &mut self,
        reader: TakeCell<TextStreamReader>,
        topic: String,
        identity: ParticipantIdentity,
        service: &LkService,
    ) {
        let Some(entry) = self.subscriptions.get(&(topic, StreamKind::Text)).cloned() else {
            return;
        };
        let Some(reader) = reader.take() else {
            return;
        };
        service.runtime().spawn(async move {
            let compressed = reader.info().compressed;
            let inline = reader.info().inline;
            let (size, preview) = match reader.read_all().await {
                Ok(text) => (text.as_bytes().len(), truncate_chars(&text, PREVIEW_CHARS)),
                Err(e) => (0, format!("<error: {}>", e)),
            };
            push_received(&entry, identity.as_str(), size, compressed, inline, preview);
        });
    }

    /// Routes an incoming byte stream to a matching subscription (if any).
    pub fn on_byte_stream(
        &mut self,
        reader: TakeCell<ByteStreamReader>,
        topic: String,
        identity: ParticipantIdentity,
        service: &LkService,
    ) {
        let Some(entry) = self.subscriptions.get(&(topic, StreamKind::Bytes)).cloned() else {
            return;
        };
        let Some(reader) = reader.take() else {
            return;
        };
        service.runtime().spawn(async move {
            let compressed = reader.info().compressed;
            let inline = reader.info().inline;
            let (size, preview) = match reader.read_all().await {
                Ok(data) => (data.len(), bytes_preview(data.as_ref())),
                Err(e) => (0, format!("<error: {}>", e)),
            };
            push_received(&entry, identity.as_str(), size, compressed, inline, preview);
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui, service: &LkService, room: &Arc<Room>) {
        self.show_send(ui, service, room);
        ui.add_space(8.0);
        ui.separator();
        self.show_subscribe(ui);
        self.show_subscription_cards(ui);
    }

    fn show_send(&mut self, ui: &mut egui::Ui, service: &LkService, room: &Arc<Room>) {
        ui.label(egui::RichText::new("Send Data Stream").strong());

        ui.horizontal(|ui| {
            ui.label("Kind:");
            ui.radio_value(&mut self.send_kind, StreamKind::Text, "Text");
            ui.radio_value(&mut self.send_kind, StreamKind::Bytes, "Bytes");
        });

        ui.horizontal(|ui| {
            ui.label("Topic:");
            ui.add(egui::TextEdit::singleline(&mut self.send_topic).desired_width(f32::INFINITY));
        });

        // Destination picker: broadcast (None) or a specific remote participant.
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
            let combo_label = self
                .send_destination
                .as_ref()
                .map(|i| i.as_str().to_string())
                .unwrap_or_else(|| "Everyone (broadcast)".to_string());
            egui::ComboBox::from_id_salt("ds_dest_combo").selected_text(combo_label).show_ui(
                ui,
                |ui| {
                    ui.selectable_value(&mut self.send_destination, None, "Everyone (broadcast)");
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
            ui.label("Content:");
            if self.send_kind == StreamKind::Text {
                if ui.small_button("Hello").clicked() {
                    self.send_content = "Hello world".to_string();
                }
                if ui.small_button("20k").clicked() {
                    self.send_content = "X".repeat(20_000);
                }
            } else {
                ui.checkbox(&mut self.send_hex, "Hex");
            }
        });
        let max_h = ui.text_style_height(&egui::TextStyle::Body) * 5.0 + 8.0;
        egui::ScrollArea::vertical().id_salt("ds_send_content_scroll").max_height(max_h).show(
            ui,
            |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.send_content)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
            },
        );

        let sending = matches!(&*self.send_state.lock(), SendState::Sending);
        let can_send = !sending && !self.send_topic.trim().is_empty();

        ui.horizontal(|ui| {
            ui.add_enabled_ui(can_send, |ui| {
                if ui.button("Send").clicked() {
                    self.dispatch_send(service, room);
                }
            });
            if sending {
                ui.spinner();
            }
        });

        match &*self.send_state.lock() {
            SendState::Sending => {
                ui.colored_label(Color32::GRAY, "Sending...");
            }
            SendState::Ok(s) => {
                ui.colored_label(Color32::LIGHT_GREEN, format!("OK: {}", s));
            }
            SendState::Err(e) => {
                ui.colored_label(Color32::LIGHT_RED, format!("Error: {}", e));
            }
            SendState::Idle => {}
        }
    }

    fn dispatch_send(&mut self, service: &LkService, room: &Arc<Room>) {
        let topic = self.send_topic.trim().to_string();
        let destination_identities =
            self.send_destination.as_ref().map(|i| vec![i.clone()]).unwrap_or_default();
        let local = room.local_participant();
        let state = self.send_state.clone();

        match self.send_kind {
            StreamKind::Text => {
                let text = self.send_content.clone();
                let options =
                    StreamTextOptions { topic, destination_identities, ..Default::default() };
                *state.lock() = SendState::Sending;
                service.runtime().spawn(async move {
                    let result = local.send_text(&text, options).await;
                    *state.lock() = match result {
                        Ok(info) => SendState::Ok(format!("stream {}", short_id(&info.id))),
                        Err(e) => SendState::Err(e.to_string()),
                    };
                });
            }
            StreamKind::Bytes => {
                let bytes = if self.send_hex {
                    match parse_hex(&self.send_content) {
                        Ok(b) => b,
                        Err(e) => {
                            *state.lock() = SendState::Err(format!("invalid hex: {}", e));
                            return;
                        }
                    }
                } else {
                    self.send_content.as_bytes().to_vec()
                };
                let options =
                    StreamByteOptions { topic, destination_identities, ..Default::default() };
                *state.lock() = SendState::Sending;
                service.runtime().spawn(async move {
                    let result = local.send_bytes(bytes, options).await;
                    *state.lock() = match result {
                        Ok(info) => SendState::Ok(format!("stream {}", short_id(&info.id))),
                        Err(e) => SendState::Err(e.to_string()),
                    };
                });
            }
        }
    }

    fn show_subscribe(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Subscriptions").strong());

        let mut do_add = false;
        ui.horizontal(|ui| {
            ui.label("Topic:");
            let resp = ui.add(egui::TextEdit::singleline(&mut self.sub_topic).desired_width(120.0));
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                do_add = true;
            }
            ui.radio_value(&mut self.sub_kind, StreamKind::Text, "Text");
            ui.radio_value(&mut self.sub_kind, StreamKind::Bytes, "Bytes");
            if ui.button("Add").clicked() {
                do_add = true;
            }
        });

        if do_add {
            self.sub_error = None;
            let topic = self.sub_topic.trim().to_string();
            let key = (topic.clone(), self.sub_kind);
            if topic.is_empty() {
                self.sub_error = Some("Topic is empty".to_string());
            } else if self.subscriptions.contains_key(&key) {
                self.sub_error =
                    Some(format!("Already subscribed to '{}' ({})", topic, self.sub_kind.label()));
            } else {
                self.subscriptions.insert(
                    key,
                    Arc::new(Mutex::new(TopicEntry {
                        topic,
                        kind: self.sub_kind,
                        received: VecDeque::new(),
                        count: 0,
                    })),
                );
                self.sub_topic.clear();
            }
        }

        if let Some(err) = &self.sub_error {
            ui.colored_label(Color32::LIGHT_RED, err);
        }
    }

    fn show_subscription_cards(&mut self, ui: &mut egui::Ui) {
        let keys: Vec<(String, StreamKind)> = self.subscriptions.keys().cloned().collect();
        let mut to_remove: Option<(String, StreamKind)> = None;

        for key in keys {
            let entry = self.subscriptions.get(&key).unwrap().clone();
            ui.add_space(6.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                let guard = entry.lock();
                ui.horizontal(|ui| {
                    ui.monospace(egui::RichText::new(&guard.topic).strong());
                    ui.label(
                        egui::RichText::new(format!("[{}]", guard.kind.label()))
                            .small()
                            .color(Color32::GRAY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Remove").clicked() {
                            to_remove = Some(key.clone());
                        }
                    });
                });

                ui.label(format!("Received ({})", guard.count));
                if guard.received.is_empty() {
                    ui.colored_label(Color32::GRAY, "Nothing received yet");
                } else {
                    let max_h = ui.text_style_height(&egui::TextStyle::Body) * 10.0 + 8.0;
                    egui::ScrollArea::vertical()
                        .id_salt(format!("ds_recv_scroll_{}_{}", guard.topic, guard.kind.label()))
                        .max_height(max_h)
                        .show(ui, |ui| {
                            for r in guard.received.iter() {
                                let meta = format!(
                                    "#{} | {} | {} | {}",
                                    r.n,
                                    r.sender,
                                    format_size(r.size),
                                    format_ts(r.received_at),
                                );
                                ui.add(egui::Label::new(
                                    egui::RichText::new(meta).small().color(Color32::GRAY),
                                ));
                                ui.horizontal(|ui| {
                                    flag_label(ui, "inline", r.inline);
                                    flag_label(ui, "compressed", r.compressed);
                                });
                                ui.add(egui::Label::new(
                                    egui::RichText::new(&r.preview).monospace(),
                                ));
                                ui.separator();
                            }
                        });
                }
            });
        }

        if let Some(key) = to_remove {
            self.subscriptions.remove(&key);
        }
    }
}

fn push_received(
    entry: &Arc<Mutex<TopicEntry>>,
    sender: &str,
    size: usize,
    compressed: bool,
    inline: bool,
    preview: String,
) {
    let mut g = entry.lock();
    g.count += 1;
    let n = g.count;
    g.received.push_back(ReceivedStream {
        n,
        sender: sender.to_string(),
        received_at: SystemTime::now(),
        size,
        compressed,
        inline,
        preview,
    });
    while g.received.len() > MAX_RECEIVED {
        g.received.pop_front();
    }
}

/// Parses a hex string into bytes, ignoring whitespace, commas and colons.
fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let cleaned: String =
        s.chars().filter(|c| !c.is_whitespace() && *c != ',' && *c != ':').collect();
    if cleaned.len() % 2 != 0 {
        return Err("odd number of hex digits".to_string());
    }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let pair = &cleaned[i..i + 2];
        let byte =
            u8::from_str_radix(pair, 16).map_err(|_| format!("invalid hex byte '{}'", pair))?;
        out.push(byte);
        i += 2;
    }
    Ok(out)
}

/// Renders `<name>: ✓` (green) or `<name>: ✗` (red) for a boolean v2 flag.
fn flag_label(ui: &mut egui::Ui, name: &str, value: bool) {
    let (mark, color) =
        if value { ("✓", Color32::LIGHT_GREEN) } else { ("✗", Color32::LIGHT_RED) };
    ui.add(egui::Label::new(
        egui::RichText::new(format!("{}: {}", name, mark)).small().color(color),
    ));
}

fn bytes_preview(data: &[u8]) -> String {
    let shown = &data[..data.len().min(PREVIEW_BYTES)];
    let hex: String = shown.iter().map(|b| format!("{:02x} ", b)).collect();
    let ellipsis = if data.len() > PREVIEW_BYTES { "..." } else { "" };
    let text = String::from_utf8_lossy(shown);
    format!("hex: {}{}\nutf8: {}{}", hex.trim_end(), ellipsis, text, ellipsis)
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

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
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
