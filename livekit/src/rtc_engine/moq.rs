// Copyright 2026 LiveKit, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use futures_util::{stream::FuturesUnordered, StreamExt};
use livekit_datatrack::api::PublishError;
use livekit_protocol as proto;
use moqtap_client::{
    draft23::connection::{
        ClientConfig, Connection, ConnectionError, RequestStream, TransportType, REQUEST_CANCELLED,
    },
    transport::{quic::QuicTransport, Transport, TransportError},
};
use moqtap_codec::{
    draft23::{
        data_stream::{DatagramHeader, SubgroupHeader, SubgroupObject, SubgroupObjectReader},
        message::ControlMessage,
    },
    kvp::{KeyValuePair, KvpValue},
    types::TrackNamespace,
    varint::VarInt,
    version::DraftVersion,
};
use quinn::crypto::rustls::QuicClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;
use tokio::sync::{mpsc::WeakUnboundedSender, watch};
use url::{Position, Url};

use super::{
    dc_sender::{packet_handle, DataTrackSendQueue},
    rtc_session::SessionEvent,
    EngineError, EngineResult,
};

pub(super) const DEFAULT_DATA_PRIORITY: u8 = 128;

mod datatrack;
mod media;
mod network;
const MAX_MEDIA: usize = 4 << 20;

pub(super) enum Command {
    DataPublished {
        name: String,
        handle: u16,
        sid: String,
        result: tokio::sync::oneshot::Sender<std::result::Result<(), PublishError>>,
    },
    DataUnpublished {
        handle: u16,
        sid: String,
    },
    DataHandles(HashMap<u16, String>),
    Publish {
        sid: String,
        track: crate::prelude::LocalTrack,
        options: crate::options::TrackPublishOptions,
        result: tokio::sync::oneshot::Sender<EngineResult<()>>,
    },
    Unpublish(String),
    #[cfg(feature = "__lk-e2e-test")]
    Handoff(Option<IpAddr>, tokio::sync::oneshot::Sender<io::Result<()>>),
    Permissions {
        can_publish_data: bool,
        can_subscribe: bool,
    },
}

struct MediaRequest {
    request: RequestStream,
    task: tokio::task::AbortHandle,
}

impl MediaRequest {
    fn stop(&mut self, conn: &mut Connection) {
        self.task.abort();
        cancel_request(conn, &mut self.request);
    }
}

// Dropping a RequestStream resets QUIC but leaves the request live in moqtap's endpoint.
fn cancel_request(conn: &mut Connection, request: &mut RequestStream) {
    if let Err(err) = conn.cancel_request_stream(request, REQUEST_CANCELLED) {
        log::debug!("could not retire MoQ request: {err}");
    }
}

// A track-level rejection leaves the MoQ session available for other requests.
enum Attempt<T> {
    Accepted(T),
    Rejected(String),
}

impl<T> Attempt<T> {
    fn required(self) -> Result<T> {
        match self {
            Self::Accepted(value) => Ok(value),
            Self::Rejected(reason) => Err(reason.into()),
        }
    }
}

struct Remote {
    participant_id: crate::id::ParticipantSid,
    track_id: crate::id::TrackSid,
    subscription: Option<RemoteSubscription>,
}

struct RemoteSubscription {
    alias: VarInt,
    frames: tokio::sync::mpsc::Sender<media::Sample>,
    media: MediaRequest,
}

impl Remote {
    fn detach(&mut self, conn: &mut Connection, emitter: &WeakUnboundedSender<SessionEvent>) {
        log::debug!("detaching MoQ rendition {}", self.track_id);
        if let Some(subscription) = &mut self.subscription {
            subscription.media.stop(conn);
        }
        if let Some(emitter) = emitter.upgrade() {
            let _ = emitter.send(SessionEvent::MoqMedia {
                participant_id: self.participant_id.clone(),
                track_id: self.track_id.clone(),
                track: None,
            });
        }
    }
}

// Keep stream operations bounded while allowing QUIC to validate a new path and retransmit.
const STREAM_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct DataUpload {
    name: String,
    sid: String,
    request: RequestStream,
    alias: VarInt,
    next_group: u64,
}

struct DataDownload {
    handle: u16,
    sid: String,
    request: RequestStream,
    assembler: datatrack::Assembler,
}

/// A session is owned by one room and released on replacement or disconnect.
pub(super) struct MoqSession {
    namespace: TrackNamespace,
    catalog: Option<(RequestStream, VarInt)>,
    raw: quinn::Connection,
    path: network::NetworkPath,
    conn: Connection,
    uploads: HashMap<u16, DataUpload>,
    can_publish_data: bool,
    next_alias: u64,
    downloads: HashMap<VarInt, DataDownload>,
}

impl MoqSession {
    /// Attach the QUIC socket to the participant authenticated over WebSocket.
    pub(super) async fn from_join(
        join: &proto::JoinResponse,
        timeout: Duration,
    ) -> EngineResult<Self> {
        let Some((info, participant, room)) = join
            .moq
            .as_ref()
            .zip(join.participant.as_ref())
            .zip(join.room.as_ref())
            .map(|((i, p), r)| (i, p, r))
        else {
            return Err(EngineError::Connection(
                "server did not advertise a MoQ attachment".into(),
            ));
        };
        tokio::time::timeout(timeout, Self::attach(info, participant, room))
            .await
            .map_err(|_| EngineError::Connection("MoQ attachment timed out".into()))?
            .map_err(|e| EngineError::Connection(e.to_string().into()))
    }

    async fn attach(
        info: &proto::MoQTransportInfo,
        participant: &proto::ParticipantInfo,
        room: &proto::Room,
    ) -> Result<Self> {
        let mut url = Url::parse(&info.endpoint)?;
        if url.scheme() != "moqt"
            || url.path() != "/moq"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || info.participant_sid != participant.sid
            || info.profile != "livekit-moq-2;moqt=23"
            || info.namespace_prefix != "livekit"
            || info.attachment_token.is_empty()
        {
            return Err("invalid MoQ attachment description".into());
        }
        let host = match url.host().ok_or("missing MoQ endpoint host")? {
            url::Host::Ipv6(ip) => ip.to_string(),
            h => h.to_string(),
        };
        let port = url.port().ok_or("missing MoQ endpoint port")?;
        let authority = url[Position::BeforeHost..Position::AfterPort].to_owned();
        url.query_pairs_mut()
            .append_pair("room", &room.name)
            .append_pair("sid", &participant.sid)
            .append_pair("generation", &info.generation.to_string())
            .append_pair("ticket", &info.attachment_token);
        let parameters = vec![
            KeyValuePair {
                key: VarInt::from_u64(1)?,
                value: KvpValue::Bytes(url[Position::BeforePath..].as_bytes().to_vec()),
            },
            KeyValuePair {
                key: VarInt::from_u64(5)?,
                value: KvpValue::Bytes(authority.into_bytes()),
            },
        ];
        let (raw, path) = dial(&host, port).await?;
        if raw.max_datagram_size().unwrap_or_default() < 1056 {
            return Err("MoQ DataTracks require QUIC datagram support".into());
        }
        let config = ClientConfig::new(DraftVersion::Draft23, TransportType::Quic)
            .with_setup_parameters(parameters);
        let conn =
            Connection::adopt(Transport::Quic(QuicTransport::new(raw.clone())), config).await?;
        let ns = TrackNamespace(vec![
            info.namespace_prefix.as_bytes().to_vec(),
            participant.sid.as_bytes().to_vec(),
        ]);
        let grants = participant.permission.as_ref().ok_or("missing participant permissions")?;
        let mut session = Self {
            raw,
            path,
            conn,
            namespace: ns,
            uploads: HashMap::new(),
            can_publish_data: false,
            downloads: HashMap::new(),
            catalog: None,
            next_alias: 0,
        };
        session.set_permissions(grants.can_publish_data, grants.can_subscribe).await?;
        Ok(session)
    }

    fn allocate_alias(&mut self) -> Result<VarInt> {
        if self.next_alias >= 1 << 31 {
            return Err("MoQ publication aliases exhausted".into());
        }
        let alias = VarInt::from_u64(self.next_alias)?;
        self.next_alias += 1;
        Ok(alias)
    }

    async fn subscribe(&mut self, name: &[u8]) -> Result<Attempt<(RequestStream, VarInt)>> {
        let mut req = tokio::time::timeout(
            STREAM_TIMEOUT,
            self.conn.subscribe(self.namespace.clone(), name.to_vec(), vec![]),
        )
        .await??;
        let response =
            tokio::time::timeout(STREAM_TIMEOUT, self.conn.recv_on_request_stream(&mut req)).await;
        let Ok(response) = response else {
            cancel_request(&mut self.conn, &mut req);
            return Ok(Attempt::Rejected("timed out waiting for SUBSCRIBE response".into()));
        };
        let response = match response {
            Err(ConnectionError::Transport(TransportError::StreamReset(code))) => {
                return Ok(Attempt::Rejected(format!("SFU reset the subscription (code {code})")));
            }
            other => other?,
        };
        match response {
            ControlMessage::SubscribeOk(ack) => Ok(Attempt::Accepted((req, ack.track_alias))),
            ControlMessage::RequestError(err) => Ok(Attempt::Rejected(format!(
                "SFU refused subscription (code {}): {}",
                err.error_code.into_inner(),
                String::from_utf8_lossy(&err.reason_phrase)
            ))),
            _ => Err("unexpected SUBSCRIBE response".into()),
        }
    }

    async fn set_permissions(&mut self, can_publish_data: bool, can_subscribe: bool) -> Result<()> {
        self.can_publish_data = can_publish_data;
        if !can_publish_data {
            for (_, mut up) in self.uploads.drain() {
                cancel_request(&mut self.conn, &mut up.request);
            }
        }
        if !can_subscribe {
            for (_, mut down) in self.downloads.drain() {
                cancel_request(&mut self.conn, &mut down.request);
            }
            if let Some((mut request, _)) = self.catalog.take() {
                cancel_request(&mut self.conn, &mut request);
            }
        } else if self.catalog.is_none() {
            self.catalog = Some(self.subscribe(b"catalog").await?.required()?);
        }
        Ok(())
    }

    async fn publish_data(
        &mut self,
        handle: u16,
        sid: String,
        name: String,
    ) -> std::result::Result<(), PublishError> {
        if !self.can_publish_data {
            return Err(PublishError::NotAllowed);
        }
        if self.uploads.get(&handle).is_some_and(|up| up.sid == sid) {
            return Ok(());
        }
        if let Some(mut old) = self.uploads.remove(&handle) {
            cancel_request(&mut self.conn, &mut old.request);
        }

        let alias = self.allocate_alias().map_err(|_| PublishError::LimitReached)?;
        let track_path = format!("dt/up/{sid}").into_bytes();
        let mut request = tokio::time::timeout(
            REQUEST_TIMEOUT,
            self.conn.publish(self.namespace.clone(), track_path, alias, vec![], vec![]),
        )
        .await
        .map_err(|_| PublishError::Timeout)?
        .map_err(|_| PublishError::Disconnected)?;
        let reply =
            tokio::time::timeout(REQUEST_TIMEOUT, self.conn.recv_on_request_stream(&mut request))
                .await;
        let error = match reply {
            Ok(Ok(ControlMessage::RequestOk(_))) => {
                self.uploads
                    .insert(handle, DataUpload { name, sid, alias, request, next_group: 0 });
                return Ok(());
            }
            Ok(Ok(ControlMessage::RequestError(reason))) => {
                log::warn!(
                    "MoQ DataTrack {sid} refused: {}",
                    String::from_utf8_lossy(&reason.reason_phrase)
                );
                PublishError::NotAllowed
            }
            Err(_) => PublishError::Timeout,
            _ => PublishError::Disconnected,
        };
        cancel_request(&mut self.conn, &mut request);
        Err(error)
    }

    async fn sync_data_handles(&mut self, handles: &HashMap<u16, String>) -> Result<()> {
        self.downloads.retain(|_, down| {
            let keep = handles.get(&down.handle) == Some(&down.sid);
            if !keep {
                cancel_request(&mut self.conn, &mut down.request);
            }
            keep
        });
        if self.catalog.is_none() {
            return Ok(());
        }
        for (&handle, sid) in handles {
            if self.downloads.values().any(|d| d.handle == handle && d.sid == *sid) {
                continue;
            }
            let name = format!("dt/down/{sid}");
            if let Attempt::Accepted((request, alias)) = self.subscribe(name.as_bytes()).await? {
                self.downloads.insert(
                    alias,
                    DataDownload {
                        handle,
                        sid: sid.clone(),
                        request,
                        assembler: Default::default(),
                    },
                );
            }
        }
        Ok(())
    }

    /// Own all stream readers, subscriptions and codec workers until disconnect.
    pub(super) async fn run(
        mut self,
        queue: DataTrackSendQueue,
        priorities: Arc<parking_lot::RwLock<HashMap<String, u8>>>,
        emitter: WeakUnboundedSender<SessionEvent>,
        mut close: watch::Receiver<bool>,
        mut commands: tokio::sync::mpsc::Receiver<Command>,
    ) {
        let mut workers = tokio::task::JoinSet::new();
        let mut decoders = tokio::task::JoinSet::new();
        let path = self.path.clone();
        workers.spawn(async move {
            path.monitor().await;
            Ok(())
        });
        let raw = self.raw.clone();
        let mut readers = FuturesUnordered::new();
        let mut published = HashMap::new();
        let mut subscribed = HashMap::<String, Remote>::new();
        let mut data_handles = HashMap::new();
        let mut catalog_alias = self.catalog.as_ref().map(|(_, a)| *a);
        let mut last_catalog = None;
        let run = async {
            loop {
                tokio::select! {
                    _ = raw.closed() => return Err("QUIC connection closed".into()),
                    (frame, priority) = queue.recv_with(|| {
                        let priorities = priorities.read();
                        queue.pop_by_priority(|handle| self.uploads.get(&handle)
                            .and_then(|up| priorities.get(&up.name)).copied().unwrap_or(DEFAULT_DATA_PRIORITY))
                    }), if self.can_publish_data => {
                        let Some(handle) = frame.first().and_then(|p| packet_handle(p)) else { continue };
                        let Some(upload) = self.uploads.get_mut(&handle) else { continue };
                        for packet in frame {
                            datatrack::send_packet(&raw, upload.alias, upload.next_group, priority, &packet)?;
                            upload.next_group += 1;
                        }
                    }
                    incoming = raw.read_datagram() => {
                        let buf = incoming?;
                        let (header, payload) = DatagramHeader::decode_object(&mut buf.as_ref())?;
                        let Some(down) = self.downloads.get_mut(&header.track_alias) else { continue };
                        let Some(packet) = down.assembler.push(&header, payload) else { continue };
                        if packet_handle(&packet) == Some(down.handle) {
                            if let Some(tx) = emitter.upgrade() {
                                let _ = tx.send(SessionEvent::RemoteDataTrackInput(packet.into()));
                            }
                        }
                    }
                    stream = raw.accept_uni(), if readers.len() < 16 => {
                        readers.push(read_object(stream?, catalog_alias));
                    }
                    Some(read) = readers.next(), if !readers.is_empty() => {
                        let Some((header, object)) = read? else { continue };
                        let group = header.group_id.into_inner();
                        if Some(header.track_alias) == catalog_alias {
                            if object.object_id.into_inner() != 0 || last_catalog.is_some_and(|last| last >= group) {
                                continue;
                            }
                            let catalog: media::Catalog = serde_json::from_slice(&object.payload)?;
                            if catalog.version != "draft-02" {
                                return Err("invalid room media catalog".into());
                            }
                            self.sync_catalog(&mut subscribed, catalog, &emitter, &mut decoders).await?;
                            last_catalog = Some(group);
                        } else if let Some(remote) = subscribed.values().filter_map(|r| r.subscription.as_ref()).find(|s| s.alias == header.track_alias) {
                            let _ = remote.frames.try_send(media::Sample { group, object });
                        }
                    }
                    Some(cmd) = commands.recv() => match cmd {
                        Command::Publish { sid, track, options, result } => {
                            let start = self.start_publication(&sid, track, options, &mut workers);
                            let accepted = match tokio::time::timeout(REQUEST_TIMEOUT, start).await {
                                Ok(Ok(worker)) => {
                                    if let Some(mut old) = published.insert(sid, worker) { old.stop(&mut self.conn); }
                                    Ok(())
                                }
                                Ok(Err(err)) => Err(EngineError::Connection(err.to_string().into())),
                                Err(_) => Err(EngineError::Connection("MoQ publication timed out".into())),
                            };
                            let _ = result.send(accepted);
                        }
                        Command::DataPublished { handle, sid, name, result } => {
                            let _ = result.send(self.publish_data(handle, sid, name).await);
                        }
                        Command::DataUnpublished { handle, sid } => {
                            if self.uploads.get(&handle).is_some_and(|up| up.sid == sid) {
                                if let Some(mut up) = self.uploads.remove(&handle) {
                                    cancel_request(&mut self.conn, &mut up.request);
                                    queue.discard_handle(handle);
                                }
                            }
                        }
                        Command::DataHandles(handles) => {
                            self.sync_data_handles(&handles).await?;
                            data_handles = handles;
                        }
                        Command::Permissions { can_publish_data, can_subscribe } => {
                            if self.can_publish_data==can_publish_data && self.catalog.is_some()==can_subscribe {continue;}
                            if !can_publish_data || !self.can_publish_data {queue.drain();}
                            if !can_subscribe {
                                for (_,mut remote) in subscribed.drain() {remote.detach(&mut self.conn,&emitter);}
                                last_catalog=None;
                                // Keep signaled handles so a grant can restore eligible subscriptions.
                            }
                            tokio::time::timeout(STREAM_TIMEOUT,self.set_permissions(can_publish_data,can_subscribe)).await??;
                            catalog_alias=self.catalog.as_ref().map(|(_,alias)|*alias);
                            self.sync_data_handles(&data_handles).await?;
                        }
                        #[cfg(feature = "__lk-e2e-test")]
                        Command::Handoff(ip, result) => {
                            let _ = result.send(self.path.simulate_handoff(ip));
                        }
                        Command::Unpublish(sid) => {
                            if let Some(mut worker) = published.remove(&sid) { worker.stop(&mut self.conn); }
                        }
                    },
                    Some(result) = decoders.join_next(), if !decoders.is_empty() => match result {
                        Ok((alias, Err(error))) => {
                            // A stopped worker may complete after a newer catalog has
                            // withdrawn or replaced its subscription.
                            if let Some(remote) = subscribed.values_mut().find(|r|
                                r.subscription.as_ref().is_some_and(|s| s.alias == alias)
                            ) {
                                if let Some(mut subscription) = remote.subscription.take() {
                                    cancel_request(&mut self.conn, &mut subscription.media.request);
                                }
                                if let Some(tx) = emitter.upgrade() {
                                    let _ = tx.send(SessionEvent::MoqMedia {
                                        participant_id: remote.participant_id.clone(),
                                        track_id: remote.track_id.clone(),
                                        track: Some(Err(crate::track::TrackError::MoqSubscription(format!("decoder stopped: {error}")))),
                                    });
                                }
                            }
                        }
                        Err(err) if err.is_panic() => return Err(err.into()),
                        _ => {}
                    },
                    Some(result) = workers.join_next(), if !workers.is_empty() => match result {
                        Ok(Err(err)) => return Err(err),
                        Err(err) if err.is_panic() => return Err(err.into()),
                        _ => {}
                    },
                    control = self.conn.recv_and_dispatch() => {
                        if matches!(control?, ControlMessage::GoAway(_)) {
                            return Err("MoQ server is draining".into());
                        }
                    }
                }
            }
        };
        let result: Result<()> = tokio::select! {res=run=>res,_=close.changed()=>Ok(())};
        raw.close(0u32.into(), b"room attachment ended");
        decoders.shutdown().await;
        workers.shutdown().await;
        if let Err(err) = result {
            if !*close.borrow() {
                log::debug!("room MoQ transport ended: {err}");
                if let Some(emitter) = emitter.upgrade() {
                    let _ = emitter.send(SessionEvent::Close {
                        source: "MoQ transport".into(),
                        reason: proto::DisconnectReason::UnknownReason,
                        reconnect_reason: proto::ReconnectReason::RrSubscriberFailed,
                        action: proto::leave_request::Action::Reconnect,
                        retry_now: false,
                    });
                }
            }
        }
    }

    async fn start_publication(
        &mut self,
        sid: &str,
        track: crate::prelude::LocalTrack,
        options: crate::options::TrackPublishOptions,
        workers: &mut tokio::task::JoinSet<Result<()>>,
    ) -> Result<MediaRequest> {
        let encoder = media::Encoder::new(&track, &options)?;
        let codec = match track {
            crate::prelude::LocalTrack::Audio(_) => "opus",
            crate::prelude::LocalTrack::Video(_) => options.video_codec.as_str(),
        };
        let name = format!("up/{sid}/{codec}");
        let alias = self.allocate_alias()?;
        let mut req = self
            .conn
            .publish(self.namespace.clone(), name.into_bytes(), alias, vec![], vec![])
            .await?;
        if !matches!(
            self.conn.recv_on_request_stream(&mut req).await?,
            ControlMessage::RequestOk(_)
        ) {
            return Err("SFU refused media publication".into());
        }
        Ok(MediaRequest { request: req, task: workers.spawn(encoder.run(self.raw.clone(), alias)) })
    }

    async fn sync_catalog(
        &mut self,
        subscribed: &mut HashMap<String, Remote>,
        catalog: media::Catalog,
        emitter: &WeakUnboundedSender<SessionEvent>,
        decoders: &mut tokio::task::JoinSet<(VarInt, Result<()>)>,
    ) -> Result<()> {
        subscribed.retain(|name, r| {
            let keep = catalog.tracks.iter().any(|t| &t.name == name);
            if !keep {
                r.detach(&mut self.conn, emitter);
            }
            keep
        });
        for t in catalog.tracks {
            let Some(ids) = &t.livekit else { continue };
            let Ok(track_id) = crate::id::TrackSid::try_from(ids.sid.clone()) else {
                continue;
            };
            let Ok(participant_id) = crate::id::ParticipantSid::try_from(ids.publisher_sid.clone())
            else {
                continue;
            };
            let previous = subscribed.get(&t.name);
            if previous.is_some_and(|r| r.subscription.is_some()) {
                continue;
            }
            // Retry a refused rendition on a new catalog (a room subscription slot
            // may have opened), but report a continuous failure only once.
            let reported = previous.is_some();
            let (track, subscription) =
                match self.subscribe_rendition(&t, &ids.sid, decoders).await? {
                    Attempt::Accepted((track, subscription)) => (Ok(track), Some(subscription)),
                    Attempt::Rejected(_) if reported => continue,
                    Attempt::Rejected(reason) => {
                        (Err(crate::track::TrackError::MoqSubscription(reason)), None)
                    }
                };
            subscribed.insert(
                t.name,
                Remote {
                    participant_id: participant_id.clone(),
                    track_id: track_id.clone(),
                    subscription,
                },
            );
            if let Some(emitter) = emitter.upgrade() {
                let _ = emitter.send(SessionEvent::MoqMedia {
                    participant_id,
                    track_id,
                    track: Some(track),
                });
            }
        }
        Ok(())
    }

    // The outer error ends the attachment. A rendition rejection only affects its track.
    async fn subscribe_rendition(
        &mut self,
        t: &media::Rendition,
        sid: &str,
        decoders: &mut tokio::task::JoinSet<(VarInt, Result<()>)>,
    ) -> Result<Attempt<(libwebrtc::media_stream_track::MediaStreamTrack, RemoteSubscription)>>
    {
        if t.packaging != "loc" || (t.codec != "opus" && media::video_codec(&t.codec).is_none()) {
            return Ok(Attempt::Rejected(format!(
                "unsupported packaging {} or codec {}",
                t.packaging, t.codec
            )));
        }
        let (mut req, alias) = match self.subscribe(t.name.as_bytes()).await? {
            Attempt::Accepted(subscription) => subscription,
            Attempt::Rejected(reason) => return Ok(Attempt::Rejected(reason)),
        };
        let (decoder, track) = match media::Decoder::new(sid, t) {
            Ok(pair) => pair,
            Err(err) => {
                cancel_request(&mut self.conn, &mut req);
                return Ok(Attempt::Rejected(format!("cannot initialize decoder: {err}")));
            }
        };
        let (frames, rx) = tokio::sync::mpsc::channel(8);
        let task = decoders.spawn(async move { (alias, decoder.run(rx).await) });
        Ok(Attempt::Accepted((
            track,
            RemoteSubscription { alias, frames, media: MediaRequest { request: req, task } },
        )))
    }
}

impl Drop for MoqSession {
    fn drop(&mut self) {
        self.raw.close(0u32.into(), b"room attachment ended");
    }
}

async fn dial(host: &str, port: u16) -> Result<(quinn::Connection, network::NetworkPath)> {
    let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])?
    .with_platform_verifier()?
    .with_no_client_auth();
    #[cfg(all(feature = "__lk-e2e-test", not(target_os = "android")))]
    if let Some(path) = std::env::var_os("LIVEKIT_MOQ_TEST_CA_DER") {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(std::fs::read(path)?.into())?;
        tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_root_certificates(roots)
        .with_no_client_auth();
    }
    tls.alpn_protocols = vec![b"moqt-23".to_vec()];
    let mut config = quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(tls)?));
    let mut transport = quinn::TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(10)));
    config.transport_config(Arc::new(transport));
    let mut last_error = None;
    for addr in tokio::net::lookup_host((host, port)).await? {
        let bind = if addr.is_ipv4() {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        } else {
            IpAddr::V6(Ipv6Addr::UNSPECIFIED)
        };
        let mut endpoint = quinn::Endpoint::client(SocketAddr::new(bind, 0))?;
        endpoint.set_default_client_config(config.clone());
        let connecting = endpoint.connect(addr, host)?;
        let path = network::NetworkPath::new(endpoint, addr);
        match connecting.await {
            Ok(conn) => return Ok((conn, path)),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error
        .map_or_else(|| io::Error::other("MoQ endpoint has no addresses"), io::Error::other)
        .into())
}

async fn open_stream(
    raw: &quinn::Connection,
) -> Result<moqtap_client::draft23::connection::FramedSendStream> {
    Ok(moqtap_client::draft23::connection::FramedSendStream::new(
        moqtap_client::transport::SendStream::Quic(raw.open_uni().await?),
        DraftVersion::Draft23,
    ))
}

async fn read_object(
    mut stream: quinn::RecvStream,
    catalog: Option<VarInt>,
) -> Result<Option<(SubgroupHeader, SubgroupObject)>> {
    let header = tokio::time::timeout(STREAM_TIMEOUT, async {
        // Classify the stream before reserving anything based on a wire length.
        let mut prefix = [0u8; 64];
        let mut n = 0;
        loop {
            if n == prefix.len() {
                return Err::<_, Box<dyn std::error::Error + Send + Sync>>(
                    "invalid subgroup header".into(),
                );
            }
            stream.read_exact(&mut prefix[n..n + 1]).await?;
            n += 1;
            if let Ok(header) = SubgroupHeader::decode(&mut &prefix[..n]) {
                return Ok(header);
            }
        }
    })
    .await??;
    let limit = if Some(header.track_alias) == catalog { 256 << 10 } else { MAX_MEDIA };
    let media = Some(header.track_alias) != catalog;
    let data = match tokio::time::timeout(STREAM_TIMEOUT, stream.read_to_end(limit + 128)).await {
        Ok(Ok(bytes)) => bytes,
        // A missing video object can recover at the next keyframe.
        Err(_) | Ok(Err(quinn::ReadToEndError::Read(quinn::ReadError::Reset(_)))) if media => {
            return Ok(None)
        }
        Ok(Err(err)) => return Err(err.into()),
        Err(err) => return Err(err.into()),
    };
    let mut bytes = data.as_slice();
    let obj = SubgroupObjectReader::new(&header).read_object(&mut bytes)?;
    if !bytes.is_empty() || obj.payload.is_empty() || obj.payload.len() > limit {
        return Err("invalid room object".into());
    }
    Ok(Some((header, obj)))
}
