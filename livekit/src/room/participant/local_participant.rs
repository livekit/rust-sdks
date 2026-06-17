// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    collections::HashMap,
    fmt::Debug,
    future::Future,
    path::Path,
    pin::Pin,
    sync::{Arc, Weak},
    time::Duration,
};

use super::{
    ConnectionQuality, ParticipantInner, ParticipantKind, ParticipantKindDetail, ParticipantState,
    ParticipantTrackPermission,
};
use crate::{
    data_stream::{
        ByteStreamInfo, ByteStreamWriter, StreamByteOptions, StreamResult, StreamTextOptions,
        TextStreamInfo, TextStreamWriter,
    },
    data_track::{self, DataTrack, DataTrackOptions, DataTrackSchemaId, Local},
    e2ee::EncryptionType,
    options::{self, compute_video_encodings, video_layers_from_encodings, TrackPublishOptions},
    prelude::*,
    room::rpc::{RpcError, RpcErrorCode, RpcInvocationData},
    rtc_engine::lk_runtime::LkRuntime,
    rtc_engine::{EngineError, EngineResult, RtcEngine},
    ChatMessage, DataPacket, RoomSession, SipDTMF, Transcription,
};
use bytes::Bytes;
use chrono::Utc;
use libwebrtc::{
    native::{create_random_uuid, packet_trailer},
    rtp_parameters::RtpEncodingParameters,
    video_source::RtcVideoSource,
};
use livekit_api::signal_client::SignalError;
use livekit_protocol as proto;
use livekit_runtime::timeout;
use parking_lot::{Mutex, RwLock};
use proto::request_response::Reason;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

type LocalTrackPublishedHandler = Box<dyn Fn(LocalParticipant, LocalTrackPublication) + Send>;
type LocalTrackUnpublishedHandler = Box<dyn Fn(LocalParticipant, LocalTrackPublication) + Send>;

fn needs_video_sender_transformer(
    options: &TrackPublishOptions,
    has_publish_timing_subscribers: bool,
) -> bool {
    !options.packet_trailer_features.is_empty() || has_publish_timing_subscribers
}

#[derive(Default)]
struct LocalEvents {
    local_track_published: Mutex<Option<LocalTrackPublishedHandler>>,
    local_track_unpublished: Mutex<Option<LocalTrackUnpublishedHandler>>,
}

struct LocalInfo {
    events: LocalEvents,
    encryption_type: EncryptionType,
    all_participants_allowed: Mutex<bool>,
    track_permissions: Mutex<Vec<ParticipantTrackPermission>>,
    session: RwLock<Option<Weak<RoomSession>>>,
}

#[derive(Clone)]
pub struct LocalParticipant {
    inner: Arc<ParticipantInner>,
    local: Arc<LocalInfo>,
}

impl Debug for LocalParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalParticipant")
            .field("sid", &self.sid())
            .field("identity", &self.identity())
            .field("name", &self.name())
            .field("state", &self.state())
            .finish()
    }
}

impl LocalParticipant {
    pub(crate) fn new(
        rtc_engine: Arc<RtcEngine>,
        kind: ParticipantKind,
        kind_details: Vec<ParticipantKindDetail>,
        sid: ParticipantSid,
        identity: ParticipantIdentity,
        name: String,
        state: ParticipantState,
        metadata: String,
        attributes: HashMap<String, String>,
        joined_at: i64,
        encryption_type: EncryptionType,
        permission: Option<proto::ParticipantPermission>,
        client_protocol: i32,
    ) -> Self {
        Self {
            inner: super::new_inner(
                rtc_engine,
                sid,
                identity,
                name,
                state,
                metadata,
                attributes,
                kind,
                kind_details,
                joined_at,
                permission,
                client_protocol,
            ),
            local: Arc::new(LocalInfo {
                events: LocalEvents::default(),
                encryption_type,
                all_participants_allowed: Mutex::new(true),
                track_permissions: Mutex::new(vec![]),
                session: Default::default(),
            }),
        }
    }

    pub(crate) fn set_session(&self, session: Weak<RoomSession>) {
        *self.local.session.write() = Some(session);
    }

    pub(crate) fn session(&self) -> Option<Arc<RoomSession>> {
        self.local.session.read().as_ref().and_then(|s| s.upgrade())
    }

    pub(crate) fn internal_track_publications(&self) -> HashMap<TrackSid, TrackPublication> {
        self.inner.track_publications.read().clone()
    }

    pub(crate) fn update_info(&self, info: proto::ParticipantInfo) {
        super::update_info(&self.inner, &Participant::Local(self.clone()), info);
    }

    pub(crate) fn set_speaking(&self, speaking: bool) {
        super::set_speaking(&self.inner, &Participant::Local(self.clone()), speaking);
    }

    pub(crate) fn set_audio_level(&self, level: f32) {
        super::set_audio_level(&self.inner, &Participant::Local(self.clone()), level);
    }

    pub(crate) fn set_connection_quality(&self, quality: ConnectionQuality) {
        super::set_connection_quality(&self.inner, &Participant::Local(self.clone()), quality);
    }

    pub(crate) fn on_local_track_published(
        &self,
        handler: impl Fn(LocalParticipant, LocalTrackPublication) + Send + 'static,
    ) {
        *self.local.events.local_track_published.lock() = Some(Box::new(handler));
    }

    pub(crate) fn on_local_track_unpublished(
        &self,
        handler: impl Fn(LocalParticipant, LocalTrackPublication) + Send + 'static,
    ) {
        *self.local.events.local_track_unpublished.lock() = Some(Box::new(handler));
    }

    pub(crate) fn on_track_muted(
        &self,
        handler: impl Fn(Participant, TrackPublication) + Send + 'static,
    ) {
        super::on_track_muted(&self.inner, handler)
    }

    pub(crate) fn on_track_unmuted(
        &self,
        handler: impl Fn(Participant, TrackPublication) + Send + 'static,
    ) {
        super::on_track_unmuted(&self.inner, handler)
    }

    pub(crate) fn on_metadata_changed(
        &self,
        handler: impl Fn(Participant, String, String) + Send + 'static,
    ) {
        super::on_metadata_changed(&self.inner, handler)
    }

    pub(crate) fn on_name_changed(
        &self,
        handler: impl Fn(Participant, String, String) + Send + 'static,
    ) {
        super::on_name_changed(&self.inner, handler)
    }

    pub(crate) fn on_attributes_changed(
        &self,
        handler: impl Fn(Participant, HashMap<String, String>) + Send + 'static,
    ) {
        super::on_attributes_changed(&self.inner, handler)
    }

    pub(crate) fn on_permission_changed(
        &self,
        handler: impl Fn(Participant, Option<proto::ParticipantPermission>) + Send + 'static,
    ) {
        super::on_permission_changed(&self.inner, handler)
    }

    pub(crate) fn add_publication(&self, publication: TrackPublication) {
        super::add_publication(&self.inner, &Participant::Local(self.clone()), publication);
    }

    pub(crate) fn remove_publication(&self, sid: &TrackSid) -> Option<TrackPublication> {
        super::remove_publication(&self.inner, &Participant::Local(self.clone()), sid)
    }

    pub(crate) fn published_tracks_info(&self) -> Vec<proto::TrackPublishedResponse> {
        let tracks = self.track_publications();
        let mut vec = Vec::with_capacity(tracks.len());

        for p in tracks.values() {
            if let Some(track) = p.track() {
                vec.push(proto::TrackPublishedResponse {
                    cid: track.rtc_track().id(),
                    track: Some(p.proto_info()),
                });
            }
        }

        vec
    }

    /// Publishes a data track.
    ///
    /// # Returns
    ///
    /// The published data track if successful. Use [`LocalDataTrack::try_push`]
    /// to send data frames on the track.
    ///
    /// # Examples
    ///
    /// Publish a track named "my_track":
    ///
    /// ```
    /// # use livekit::prelude::*;
    /// # async fn with_room(room: Room) -> Result<(), PublishError> {
    /// let track = room
    ///     .local_participant()
    ///     .publish_data_track("my_track")
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Note: if you are self-hosting the LiveKit SFU and get [`data_track::PublishError::Timeout`],
    /// this may indicate you are using an outdated release that does not support data tracks.
    ///
    pub async fn publish_data_track(
        &self,
        options: impl Into<DataTrackOptions>,
    ) -> Result<DataTrack<Local>, data_track::PublishError> {
        self.session()
            .ok_or(PublishError::Disconnected)?
            .local_dt_input
            .publish_track(options.into())
            .await
    }

    /// Publishes a media track.
    pub async fn publish_track(
        &self,
        track: LocalTrack,
        options: TrackPublishOptions,
    ) -> RoomResult<LocalTrackPublication> {
        let disable_red = self.local.encryption_type != EncryptionType::None || !options.red;

        let mut req = proto::AddTrackRequest {
            cid: track.rtc_track().id(),
            name: track.name(),
            r#type: proto::TrackType::from(track.kind()) as i32,
            muted: track.is_muted(),
            source: proto::TrackSource::from(options.source) as i32,
            disable_dtx: !options.dtx,
            disable_red,
            encryption: proto::encryption::Type::from(self.local.encryption_type) as i32,
            stream: options.stream.clone(),
            ..Default::default()
        };

        if options.preconnect_buffer {
            req.audio_features.push(proto::AudioTrackFeature::TfPreconnectBuffer as i32);
        }

        req.packet_trailer_features =
            options.packet_trailer_features.to_proto().into_iter().map(|f| f as i32).collect();

        let mut encodings = Vec::default();
        match &track {
            LocalTrack::Video(video_track) => {
                // Get the video dimension
                // TODO(theomonnom): Use MediaStreamTrack::getSettings() on web
                let resolution = video_track.rtc_source().video_resolution();
                req.width = resolution.width;
                req.height = resolution.height;

                encodings = compute_video_encodings(req.width, req.height, &options);
                req.layers = video_layers_from_encodings(req.width, req.height, &encodings);

                // Populate simulcast_codecs so the server knows this track has
                // multiple quality layers — either real simulcast (multiple
                // RTP encodings) or SVC (one encoding with several spatial
                // layers carried inside it).
                let is_svc_multilayer = encodings.len() == 1
                    && encodings
                        .first()
                        .and_then(|e| e.scalability_mode.as_ref())
                        .map(|m| options::spatial_layers_from_scalability_mode(m) > 1)
                        .unwrap_or(false);
                if (options.simulcast && encodings.len() > 1) || is_svc_multilayer {
                    req.simulcast_codecs = vec![proto::SimulcastCodec {
                        codec: options.video_codec.as_str().to_string(),
                        cid: track.rtc_track().id(),
                        layers: req.layers.clone(),
                        ..Default::default()
                    }];
                }
            }
            LocalTrack::Audio(_audio_track) => {
                // Setup audio encoding
                let audio_encoding =
                    options.audio_encoding.as_ref().unwrap_or(&options::audio::MUSIC.encoding);

                encodings.push(RtpEncodingParameters {
                    max_bitrate: Some(audio_encoding.max_bitrate),
                    ..Default::default()
                });
            }
        }
        let track_info = self.inner.rtc_engine.add_track(req).await?;
        let publication = LocalTrackPublication::new(track_info.clone(), track.clone());
        track.update_info(track_info); // Update sid + source

        // set track for publication to listen mute/unmute events
        publication.set_track(Some(track.clone().into()));

        let transceiver =
            self.inner.rtc_engine.create_sender(track.clone(), options.clone(), encodings).await?;

        track.set_transceiver(Some(transceiver));

        if let LocalTrack::Video(video_track) = &track {
            let has_timing_subscribers = video_track.has_publish_timing_subscribers();
            if needs_video_sender_transformer(&options, has_timing_subscribers) {
                let trailers_enabled = !options.packet_trailer_features.is_empty();
                log::info!(
                    "sender frame transformer enabled for local video track {} (packet_trailer={}, publish_timing={})",
                    publication.sid(),
                    trailers_enabled,
                    has_timing_subscribers,
                );
                let sender = track.transceiver().unwrap().sender();
                let handler = packet_trailer::create_sender_handler(
                    LkRuntime::instance().pc_factory(),
                    &sender,
                );
                handler.set_enabled(trailers_enabled);
                video_track.set_packet_trailer_handler(handler.clone());

                #[cfg(not(target_arch = "wasm32"))]
                if let RtcVideoSource::Native(ref native_source) = video_track.rtc_source() {
                    native_source.set_packet_trailer_handler(handler.clone());
                }
            }
        }

        self.inner.rtc_engine.publisher_negotiation_needed();

        publication.update_publish_options(options);
        self.add_publication(TrackPublication::Local(publication.clone()));

        if let Some(local_track_published) = self.local.events.local_track_published.lock().as_ref()
        {
            local_track_published(self.clone(), publication.clone());
        }
        track.enable();

        Ok(publication)
    }

    pub async fn set_metadata(&self, metadata: String) -> RoomResult<()> {
        if let Ok(response) = timeout(REQUEST_TIMEOUT, {
            let request_id = self.inner.rtc_engine.session().signal_client().next_request_id();
            self.inner
                .rtc_engine
                .send_request(proto::signal_request::Message::UpdateMetadata(
                    proto::UpdateParticipantMetadata {
                        metadata,
                        name: self.name(),
                        attributes: Default::default(),
                        request_id,
                        ..Default::default()
                    },
                ))
                .await;
            self.inner.rtc_engine.get_response(request_id)
        })
        .await
        {
            match response.reason() {
                Reason::Ok => Ok(()),
                reason => Err(RoomError::Request { reason, message: response.message }),
            }
        } else {
            Err(RoomError::Engine(EngineError::Signal(SignalError::Timeout(
                "request timeout".into(),
            ))))
        }
    }

    pub async fn set_attributes(&self, attributes: HashMap<String, String>) -> RoomResult<()> {
        if let Ok(response) = timeout(REQUEST_TIMEOUT, {
            let request_id = self.inner.rtc_engine.session().signal_client().next_request_id();
            self.inner
                .rtc_engine
                .send_request(proto::signal_request::Message::UpdateMetadata(
                    proto::UpdateParticipantMetadata {
                        attributes,
                        metadata: self.metadata(),
                        name: self.name(),
                        request_id,
                        ..Default::default()
                    },
                ))
                .await;
            self.inner.rtc_engine.get_response(request_id)
        })
        .await
        {
            match response.reason() {
                Reason::Ok => Ok(()),
                reason => Err(RoomError::Request { reason, message: response.message }),
            }
        } else {
            Err(RoomError::Engine(EngineError::Signal(SignalError::Timeout(
                "request timeout".into(),
            ))))
        }
    }

    pub async fn set_name(&self, name: String) -> RoomResult<()> {
        if let Ok(response) = timeout(REQUEST_TIMEOUT, {
            let request_id = self.inner.rtc_engine.session().signal_client().next_request_id();
            self.inner
                .rtc_engine
                .send_request(proto::signal_request::Message::UpdateMetadata(
                    proto::UpdateParticipantMetadata {
                        name,
                        metadata: self.metadata(),
                        attributes: Default::default(),
                        request_id,
                        ..Default::default()
                    },
                ))
                .await;
            self.inner.rtc_engine.get_response(request_id)
        })
        .await
        {
            match response.reason() {
                Reason::Ok => Ok(()),
                reason => Err(RoomError::Request { reason, message: response.message }),
            }
        } else {
            Err(RoomError::Engine(EngineError::Signal(SignalError::Timeout(
                "request timeout".into(),
            ))))
        }
    }

    pub async fn send_chat_message(
        &self,
        text: String,
        destination_identities: Option<Vec<String>>,
        sender_identity: Option<String>,
    ) -> RoomResult<ChatMessage> {
        let chat_message = proto::ChatMessage {
            id: create_random_uuid(),
            timestamp: Utc::now().timestamp_millis(),
            message: text,
            ..Default::default()
        };

        let data = proto::DataPacket {
            value: Some(proto::data_packet::Value::ChatMessage(chat_message.clone())),
            participant_identity: sender_identity.unwrap_or_default(),
            destination_identities: destination_identities.unwrap_or_default(),
            ..Default::default()
        };

        match self.inner.rtc_engine.publish_data(data, DataPacketKind::Reliable, false).await {
            Ok(_) => Ok(ChatMessage::from(chat_message)),
            Err(e) => Err(Into::into(e)),
        }
    }

    pub async fn edit_chat_message(
        &self,
        edit_text: String,
        original_message: ChatMessage,
        destination_identities: Option<Vec<String>>,
        sender_identity: Option<String>,
    ) -> RoomResult<ChatMessage> {
        let edited_message = ChatMessage {
            message: edit_text,
            edit_timestamp: Utc::now().timestamp_millis().into(),
            ..original_message
        };
        let proto_msg = proto::ChatMessage::from(edited_message);
        let data = proto::DataPacket {
            value: Some(proto::data_packet::Value::ChatMessage(proto_msg.clone())),
            participant_identity: sender_identity.unwrap_or_default(),
            destination_identities: destination_identities.unwrap_or_default(),
            ..Default::default()
        };

        match self.inner.rtc_engine.publish_data(data, DataPacketKind::Reliable, false).await {
            Ok(_) => Ok(ChatMessage::from(proto_msg)),
            Err(e) => Err(Into::into(e)),
        }
    }

    pub async fn unpublish_track(
        &self,
        sid: &TrackSid,
        // _stop_on_unpublish: bool,
    ) -> RoomResult<LocalTrackPublication> {
        let publication = self.remove_publication(sid);
        if let Some(TrackPublication::Local(publication)) = publication {
            let track = publication.track().unwrap();
            let sender = track.transceiver().unwrap().sender();

            self.inner.rtc_engine.remove_track(sender)?;
            track.set_transceiver(None);

            if let Some(local_track_unpublished) =
                self.local.events.local_track_unpublished.lock().as_ref()
            {
                local_track_unpublished(self.clone(), publication.clone());
            }

            publication.set_track(None);
            self.inner.rtc_engine.publisher_negotiation_needed();

            Ok(publication)
        } else {
            Err(RoomError::Internal("track not found".to_string()))
        }
    }

    /** internal */
    pub async fn publish_raw_data(
        self,
        packet: proto::DataPacket,
        reliable: bool,
    ) -> RoomResult<()> {
        let kind = match reliable {
            true => DataPacketKind::Reliable,
            false => DataPacketKind::Lossy,
        };
        self.inner.rtc_engine.publish_data(packet, kind, true).await.map_err(Into::into)
    }

    /// Publishes a data packet.
    pub async fn publish_data(&self, packet: DataPacket) -> RoomResult<()> {
        let kind = match packet.reliable {
            true => DataPacketKind::Reliable,
            false => DataPacketKind::Lossy,
        };
        let destination_identities: Vec<String> =
            packet.destination_identities.into_iter().map(Into::into).collect();
        let data = proto::DataPacket {
            kind: kind as i32,
            destination_identities: destination_identities.clone(),
            value: Some(proto::data_packet::Value::User(proto::UserPacket {
                payload: packet.payload,
                topic: packet.topic,
                ..Default::default()
            })),
            ..Default::default()
        };

        self.inner.rtc_engine.publish_data(data, kind, false).await.map_err(Into::into)
    }

    pub fn set_data_channel_buffered_amount_low_threshold(
        &self,
        threshold: u64,
        kind: DataPacketKind,
    ) -> RoomResult<()> {
        self.inner
            .rtc_engine
            .session()
            .set_data_channel_buffered_amount_low_threshold(threshold, kind);
        Ok(())
    }

    pub fn data_channel_buffered_amount_low_threshold(
        &self,
        kind: DataPacketKind,
    ) -> RoomResult<u64> {
        Ok(self.inner.rtc_engine.session().data_channel_buffered_amount_low_threshold(kind))
    }

    pub async fn set_track_subscription_permissions(
        &self,
        all_participants_allowed: bool,
        permissions: Vec<ParticipantTrackPermission>,
    ) -> RoomResult<()> {
        *self.local.track_permissions.lock() = permissions;
        *self.local.all_participants_allowed.lock() = all_participants_allowed;
        self.update_track_subscription_permissions().await;
        Ok(())
    }

    pub async fn publish_transcription(&self, packet: Transcription) -> RoomResult<()> {
        let segments: Vec<proto::TranscriptionSegment> = packet
            .segments
            .into_iter()
            .map(|segment| proto::TranscriptionSegment {
                id: segment.id,
                start_time: segment.start_time,
                end_time: segment.end_time,
                text: segment.text,
                r#final: segment.r#final,
                language: segment.language,
            })
            .collect();
        let transcription_packet = proto::Transcription {
            transcribed_participant_identity: packet.participant_identity,
            segments: segments,
            track_id: packet.track_id,
        };
        let data = proto::DataPacket {
            value: Some(proto::data_packet::Value::Transcription(transcription_packet)),
            ..Default::default()
        };
        self.inner
            .rtc_engine
            .publish_data(data, DataPacketKind::Reliable, false)
            .await
            .map_err(Into::into)
    }

    pub async fn publish_dtmf(&self, dtmf: SipDTMF) -> RoomResult<()> {
        let destination_identities: Vec<String> =
            dtmf.destination_identities.into_iter().map(Into::into).collect();
        let dtmf_message = proto::SipDtmf { code: dtmf.code, digit: dtmf.digit };

        let data = proto::DataPacket {
            value: Some(proto::data_packet::Value::SipDtmf(dtmf_message)),
            destination_identities: destination_identities.clone(),
            ..Default::default()
        };

        self.inner
            .rtc_engine
            .publish_data(data, DataPacketKind::Reliable, false)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_track_subscription_permissions(&self) {
        let all_participants_allowed = *self.local.all_participants_allowed.lock();
        let track_permissions = self
            .local
            .track_permissions
            .lock()
            .iter()
            .map(|p| proto::TrackPermission::from(p.clone()))
            .collect();

        self.inner
            .rtc_engine
            .send_request(proto::signal_request::Message::SubscriptionPermission(
                proto::SubscriptionPermission {
                    all_participants: all_participants_allowed,
                    track_permissions,
                },
            ))
            .await;
    }

    pub fn get_track_publication(&self, sid: &TrackSid) -> Option<LocalTrackPublication> {
        self.inner.track_publications.read().get(sid).map(|track| {
            if let TrackPublication::Local(local) = track {
                return local.clone();
            }

            unreachable!()
        })
    }

    pub fn sid(&self) -> ParticipantSid {
        self.inner.info.read().sid.clone()
    }

    pub fn identity(&self) -> ParticipantIdentity {
        self.inner.info.read().identity.clone()
    }

    pub fn name(&self) -> String {
        self.inner.info.read().name.clone()
    }

    pub fn state(&self) -> ParticipantState {
        self.inner.info.read().state
    }

    pub fn metadata(&self) -> String {
        self.inner.info.read().metadata.clone()
    }

    pub fn attributes(&self) -> HashMap<String, String> {
        self.inner.info.read().attributes.clone()
    }

    pub fn is_speaking(&self) -> bool {
        self.inner.info.read().speaking
    }

    pub fn track_publications(&self) -> HashMap<TrackSid, LocalTrackPublication> {
        self.inner
            .track_publications
            .read()
            .clone()
            .into_iter()
            .map(|(sid, track)| {
                if let TrackPublication::Local(local) = track {
                    return (sid, local);
                }

                unreachable!()
            })
            .collect()
    }

    pub fn audio_level(&self) -> f32 {
        self.inner.info.read().audio_level
    }

    pub fn connection_quality(&self) -> ConnectionQuality {
        self.inner.info.read().connection_quality
    }

    pub fn kind(&self) -> ParticipantKind {
        self.inner.info.read().kind
    }

    pub fn kind_details(&self) -> Vec<ParticipantKindDetail> {
        self.inner.info.read().kind_details.clone()
    }

    pub fn disconnect_reason(&self) -> DisconnectReason {
        self.inner.info.read().disconnect_reason
    }

    pub fn joined_at(&self) -> i64 {
        self.inner.info.read().joined_at
    }

    pub fn permission(&self) -> Option<proto::ParticipantPermission> {
        self.inner.info.read().permission.clone()
    }

    pub fn client_protocol(&self) -> i32 {
        self.inner.info.read().client_protocol
    }

    pub async fn perform_rpc(&self, data: PerformRpcData) -> Result<String, RpcError> {
        let session = self.session().ok_or_else(|| {
            RpcError::built_in(RpcErrorCode::SendFailed, Some("Not connected".to_string()))
        })?;
        let transport = crate::room::rpc::SessionTransport(session.clone());
        session.rpc_client.perform_rpc(data, &transport).await
    }

    pub fn register_rpc_method(
        &self,
        method: String,
        handler: impl Fn(RpcInvocationData) -> Pin<Box<dyn Future<Output = Result<String, RpcError>> + Send>>
            + Send
            + Sync
            + 'static,
    ) {
        if let Some(session) = self.session() {
            session.rpc_server.register_method(method, handler);
        }

        // Pre-connect the publisher PC so ACKs can be sent immediately when requests arrive.
        // Without this, the first RPC request would trigger publisher negotiation, causing
        // a ~300-500ms delay before the ACK can be sent (ICE negotiation time).
        self.inner.rtc_engine.publisher_negotiation_needed();
    }

    pub fn unregister_rpc_method(&self, method: String) {
        if let Some(session) = self.session() {
            session.rpc_server.unregister_method(&method);
        }
    }

    /// Send text to participants in the room.
    ///
    /// This method sends a complete text string to participants in the room as a text stream.
    /// The text is sent in a single operation, and the method returns information about the
    /// stream used to send the text.
    ///
    /// # Arguments
    ///
    /// * `text` - The text content to send.
    /// * `options` - Configuration options for the text stream, including topic and
    ///   destination participants.
    ///
    pub async fn send_text(
        &self,
        text: &str,
        options: StreamTextOptions,
    ) -> StreamResult<TextStreamInfo> {
        self.session().unwrap().outgoing_stream_manager.send_text(text, options).await
    }

    /// Send a file on disk to participants in the room.
    ///
    /// This method reads a file from the specified path and sends its contents
    /// to participants in the room as a byte stream, and the method returns information
    /// the stream used to send the file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to be sent.
    /// * `options` - Configuration options for the byte stream, including topic and
    ///   destination participants.
    ///
    pub async fn send_file(
        &self,
        path: impl AsRef<Path>,
        options: StreamByteOptions,
    ) -> StreamResult<ByteStreamInfo> {
        self.session().unwrap().outgoing_stream_manager.send_file(path, options).await
    }

    /// Send an in-memory blob of bytes to participants in the room.
    ///
    /// This method sends a provided byte slice as a byte stream.
    ///
    /// # Arguments
    ///
    /// * `data` - The bytes to send.
    /// * `options` - Configuration options for the byte stream, including topic and
    ///   destination participants.
    pub async fn send_bytes(
        &self,
        data: impl AsRef<[u8]>,
        options: StreamByteOptions,
    ) -> StreamResult<ByteStreamInfo> {
        self.session().unwrap().outgoing_stream_manager.send_bytes(data, options).await
    }

    /// Stream text incrementally to participants in the room.
    ///
    /// This method allows sending text data in chunks as it becomes available.
    /// Unlike `send_text`, which sends the entire text at once, this method returns
    /// a writer that can be used to send text incrementally.
    ///
    /// # Arguments
    ///
    /// * `options` - Configuration options for the text stream, including topic and
    ///   destination participants.
    ///
    pub async fn stream_text(&self, options: StreamTextOptions) -> StreamResult<TextStreamWriter> {
        self.session().unwrap().outgoing_stream_manager.stream_text(options).await
    }

    /// Stream bytes incrementally to participants in the room.
    ///
    /// This method allows sending binary data in chunks as it becomes available.
    /// Unlike `send_file`, which sends the entire file at once, this method returns
    /// a writer that can be used to send binary data incrementally.
    ///
    /// # Arguments
    ///
    /// * `options` - Configuration options for the byte stream, including topic and
    ///   destination participants.
    ///
    pub async fn stream_bytes(&self, options: StreamByteOptions) -> StreamResult<ByteStreamWriter> {
        self.session().unwrap().outgoing_stream_manager.stream_bytes(options).await
    }

    pub fn is_encrypted(&self) -> bool {
        *self.inner.is_encrypted.read()
    }

    #[doc(hidden)]
    pub fn update_data_encryption_status(&self, _is_encrypted: bool) {
        // Local participants don't receive data messages, so this is a no-op
    }

    /// Stores the definition of a data track schema.
    ///
    /// Called by a publisher to make a schema available to subscribers, who can
    /// later look up its definition via [`get_schema`](Self::get_schema). Define a
    /// schema before publishing any data track that references it, so that
    /// subscribers can resolve the schema by its ID.
    ///
    /// A schema can only be defined once. Attempting to redefine an existing
    /// schema returns an error.
    ///
    /// # Arguments
    ///
    /// * `id` - Identifies the schema; the same ID is provided when publishing a
    ///   data track that uses it.
    /// * `definition` - The schema definition, stored as-is. It is neither parsed
    ///   nor validated against its [encoding](DataTrackSchemaId::encoding), so
    ///   the caller is responsible for ensuring it is well-formed.
    ///
    pub async fn define_schema(&self, id: DataTrackSchemaId, definition: String) -> RoomResult<()> {
        self.store_data_blob(id.into(), definition.into()).await?;
        Ok(())
    }

    /// Retrieves the definition for a data track schema.
    ///
    /// Called by a subscriber that wants to inspect the schema a participant
    /// [defined](Self::define_schema) for a data track it is publishing. Returns
    /// an error if the participant has not defined a schema with this ID.
    ///
    /// # Arguments
    ///
    /// * `id` - Identifies the schema to retrieve.
    /// * `participant` - Identity of the participant that defined the schema.
    ///
    pub async fn get_schema(
        &self,
        id: DataTrackSchemaId,
        participant: ParticipantIdentity,
    ) -> RoomResult<String> {
        let contents = self
            .get_data_blob(id.into(), participant)
            .await
            .map_err(|err| {
                log::error!("failed to get schema: {err}");
                err
            })?;


        let definition = String::from_utf8(contents.to_vec()).map_err(|err| {
            RoomError::Internal(format!("schema definition is not valid UTF-8: {err}"))
        })?;
        Ok(definition)
    }

    // TODO: unify request/response logic, timeout behavior across SDK.
    const DATA_BLOB_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

    /// Stores an arbitrary blob of data on the server, keyed by `key`.
    async fn store_data_blob(&self, key: proto::DataBlobKey, contents: Bytes) -> EngineResult<()> {
        let blob = proto::DataBlob { key: Some(key), contents: contents.into() };

        let session = self.inner.rtc_engine.session();
        let request_id = session.signal_client().next_request_id();

        // Success is reported via `StoreDataBlobResponse` and error via `RequestResponse`;
        // both carry the request id, so both paths are correlated by it.
        let store_ok_response = session.store_data_blob_response(request_id);
        let store_error_response = session.get_response(request_id);

        let request = proto::StoreDataBlobRequest { blob: Some(blob), request_id };
        self.inner
            .rtc_engine
            .send_request(proto::signal_request::Message::StoreDataBlobRequest(request))
            .await;

        let response = timeout(Self::DATA_BLOB_REQUEST_TIMEOUT, async {
            tokio::select! {
                _ = store_ok_response => Ok(()),
                error = store_error_response => Err(error),
            }
        })
        .await
        .map_err(|_| {
            EngineError::Signal(SignalError::Timeout("store data blob timed out".into()))
        })?;

        match response {
            Ok(()) => Ok(()),
            Err(error) => Err(EngineError::Internal(
                format!("store data blob request failed ({:?}): {}", error.reason(), error.message)
                    .into(),
            )),
        }
    }

    /// Retrieves a blob of data previously stored by `participant` under `key`.
    async fn get_data_blob(
        &self,
        key: proto::DataBlobKey,
        participant: ParticipantIdentity,
    ) -> EngineResult<Bytes> {
        let session = self.inner.rtc_engine.session();
        let request_id = session.signal_client().next_request_id();

        // Success is reported via `GetDataBlobResponse` and error via `RequestResponse`;
        // both carry the request id, so both paths are correlated by it.
        let get_ok_response = session.get_data_blob_response(request_id);
        let get_error_response = session.get_response(request_id);

        let request = proto::GetDataBlobRequest {
            key: Some(key),
            participant_identity: participant.0,
            request_id,
        };
        self.inner
            .rtc_engine
            .send_request(proto::signal_request::Message::GetDataBlobRequest(request))
            .await;

        let response = timeout(Self::DATA_BLOB_REQUEST_TIMEOUT, async {
            tokio::select! {
                response = get_ok_response => Ok(response),
                error = get_error_response => Err(error),
            }
        })
        .await
        .map_err(|_| EngineError::Signal(SignalError::Timeout("get data blob timed out".into())))?;

        match response {
            Ok(response) => {
                let blob = response.blob.ok_or_else(|| {
                    EngineError::Internal("get data blob response is malformed".into())
                })?;
                Ok(blob.contents.into())
            }
            Err(error) => Err(EngineError::Internal(
                format!("get data blob request failed ({:?}): {}", error.reason(), error.message)
                    .into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::PacketTrailerFeatures;

    #[test]
    fn timing_subscribers_request_video_sender_transformer_without_packet_trailers() {
        let options = TrackPublishOptions {
            packet_trailer_features: PacketTrailerFeatures::default(),
            ..Default::default()
        };

        assert!(needs_video_sender_transformer(&options, true));
    }

    #[test]
    fn packet_trailer_features_request_video_sender_transformer_without_timing_subscribers() {
        let options = TrackPublishOptions {
            packet_trailer_features: PacketTrailerFeatures {
                user_timestamp: true,
                frame_id: false,
            },
            ..Default::default()
        };

        assert!(needs_video_sender_transformer(&options, false));
    }

    #[test]
    fn video_sender_transformer_is_skipped_without_timing_or_packet_trailers() {
        let options = TrackPublishOptions {
            packet_trailer_features: PacketTrailerFeatures::default(),
            ..Default::default()
        };

        assert!(!needs_video_sender_transformer(&options, false));
    }
}
