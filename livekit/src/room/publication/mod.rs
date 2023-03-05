use super::track::{TrackDimension, TrackEvent};
use crate::prelude::*;
use crate::proto;
use crate::track::LocalTrack;
use crate::track::RemoteTrack;
use crate::track::Track;
use livekit_utils::enum_dispatch;
use livekit_utils::observer::Dispatcher;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub(crate) struct TrackPublicationInner {
    track: Mutex<Option<Track>>,
    name: Mutex<String>,
    sid: Mutex<TrackSid>,
    kind: AtomicU8,   // Casted to TrackKind
    source: AtomicU8, // Casted to TrackSource
    simulcasted: AtomicBool,
    dimension: Mutex<TrackDimension>,
    mime_type: Mutex<String>,
    muted: AtomicBool,
    participant: ParticipantSid,
    dispatcher: Mutex<Dispatcher<TrackEvent>>,
    close_sender: Mutex<Option<oneshot::Sender<()>>>,
}

impl TrackPublicationInner {
    pub fn new(info: proto::TrackInfo, participant: ParticipantSid, track: Option<Track>) -> Self {
        Self {
            track: Mutex::new(track),
            name: Mutex::new(info.name),
            sid: Mutex::new(info.sid.into()),
            kind: AtomicU8::new(
                TrackKind::try_from(proto::TrackType::from_i32(info.r#type).unwrap()).unwrap()
                    as u8,
            ),
            source: AtomicU8::new(TrackSource::from(
                proto::TrackSource::from_i32(info.source).unwrap(),
            ) as u8),
            simulcasted: AtomicBool::new(info.simulcast),
            dimension: Mutex::new(TrackDimension(info.width, info.height)),
            mime_type: Mutex::new(info.mime_type),
            muted: AtomicBool::new(info.muted),
            dispatcher: Default::default(),
            close_sender: Default::default(),
            participant,
        }
    }

    pub fn update_track(self: &Arc<Self>, track: Option<Track>) {
        let mut old_track = self.track.lock();

        if let Some(close_sender) = self.close_sender.lock().take() {
            let _ = close_sender.send(());
        }

        *old_track = track.clone();
        if let Some(track) = track {
            let (close_sender, close_receiver) = oneshot::channel();
            self.close_sender.lock().replace(close_sender);

            let track_receiver = track.register_observer();
            tokio::spawn(
                self.clone()
                    .publication_task(close_receiver, track_receiver),
            );
        }
    }

    pub fn update_info(&self, info: proto::TrackInfo) {
        *self.name.lock() = info.name;
        *self.sid.lock() = info.sid.into();
        *self.dimension.lock() = TrackDimension(info.width, info.height);
        *self.mime_type.lock() = info.mime_type;
        self.kind.store(
            TrackKind::try_from(proto::TrackType::from_i32(info.r#type).unwrap()).unwrap() as u8,
            Ordering::SeqCst,
        );
        self.source.store(
            TrackSource::from(proto::TrackSource::from_i32(info.source).unwrap()) as u8,
            Ordering::SeqCst,
        );
        self.simulcasted.store(info.simulcast, Ordering::SeqCst);
        self.muted.store(info.muted, Ordering::SeqCst);

        if let Some(track) = self.track.lock().as_ref() {
            track.set_muted(info.muted);
        }
    }

    /// Task used to forward TrackHandle's events to the TrackPublications's dispatcher
    async fn publication_task(
        self: Arc<Self>,
        mut close_receiver: oneshot::Receiver<()>,
        mut track_receiver: mpsc::UnboundedReceiver<TrackEvent>,
    ) {
        loop {
            tokio::select! {
                event = track_receiver.recv() => {
                    match event {
                        Some(event) => {
                            self.dispatcher.lock().dispatch(&event);
                        },
                        None => break,
                    }
                }
                _ = &mut close_receiver => {
                    break;
                }
            }
        }
    }

    #[inline]
    pub fn sid(&self) -> TrackSid {
        self.sid.lock().clone()
    }

    #[inline]
    pub fn name(&self) -> String {
        self.name.lock().clone()
    }

    #[inline]
    pub fn kind(&self) -> TrackKind {
        self.kind.load(Ordering::SeqCst).try_into().unwrap()
    }

    #[inline]
    pub fn source(&self) -> TrackSource {
        self.source.load(Ordering::SeqCst).into()
    }

    #[inline]
    pub fn simulcasted(&self) -> bool {
        self.simulcasted.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn dimension(&self) -> TrackDimension {
        self.dimension.lock().clone()
    }

    #[inline]
    pub fn mime_type(&self) -> String {
        self.mime_type.lock().clone()
    }

    #[inline]
    pub fn track(&self) -> Option<Track> {
        self.track.lock().clone()
    }

    #[inline]
    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }
}

impl Drop for TrackPublicationInner {
    fn drop(&mut self) {
        if let Some(close_sender) = self.close_sender.lock().take() {
            let _ = close_sender.send(());
        }
    }
}

#[derive(Clone, Debug)]
pub enum TrackPublication {
    Local(LocalTrackPublication),
    Remote(RemoteTrackPublication),
}

impl TrackPublication {
    enum_dispatch!(
        [Local, Remote];
        pub fn sid(self: &Self) -> TrackSid;
        pub fn name(self: &Self) -> String;
        pub fn kind(self: &Self) -> TrackKind;
        pub fn source(self: &Self) -> TrackSource;
        pub fn simulcasted(self: &Self) -> bool;
        pub fn dimension(self: &Self) -> TrackDimension;
        pub fn mime_type(self: &Self) -> String;
        pub fn muted(self: &Self) -> bool;
    );

    pub fn track(&self) -> Option<Track> {
        match self {
            TrackPublication::Local(p) => p.inner.track.lock().clone(),
            TrackPublication::Remote(p) => p.inner.track.lock().clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LocalTrackPublication {
    inner: Arc<TrackPublicationInner>,
}

impl LocalTrackPublication {
    pub fn new(info: proto::TrackInfo, participant: ParticipantSid, track: Track) -> Self {
        Self {
            inner: Arc::new(TrackPublicationInner::new(info, participant, Some(track))),
        }
    }

    pub fn sid(&self) -> TrackSid {
        self.inner.sid()
    }

    pub fn name(&self) -> String {
        self.inner.name()
    }

    pub fn kind(&self) -> TrackKind {
        self.inner.kind()
    }

    pub fn source(&self) -> TrackSource {
        self.inner.source()
    }

    pub fn simulcasted(&self) -> bool {
        self.inner.simulcasted()
    }

    pub fn dimension(&self) -> TrackDimension {
        self.inner.dimension()
    }

    pub fn track(&self) -> LocalTrack {
        self.inner.track.lock().clone().unwrap().try_into().unwrap()
    }

    pub fn mime_type(&self) -> String {
        self.inner.mime_type()
    }

    pub fn muted(&self) -> bool {
        self.inner.muted()
    }

    pub(crate) fn update_track(&self, track: Option<Track>) {
        self.inner.update_track(track);
    }

    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        self.inner.update_info(info);
    }
}

#[derive(Clone, Debug)]
pub struct RemoteTrackPublication {
    inner: Arc<TrackPublicationInner>,
}

impl RemoteTrackPublication {
    pub fn new(info: proto::TrackInfo, participant: ParticipantSid, track: Option<Track>) -> Self {
        Self {
            inner: Arc::new(TrackPublicationInner::new(info, participant, track)),
        }
    }

    pub fn sid(&self) -> TrackSid {
        self.inner.sid()
    }

    pub fn name(&self) -> String {
        self.inner.name()
    }

    pub fn kind(&self) -> TrackKind {
        self.inner.kind()
    }

    pub fn source(&self) -> TrackSource {
        self.inner.source()
    }

    pub fn simulcasted(&self) -> bool {
        self.inner.simulcasted()
    }

    pub fn dimension(&self) -> TrackDimension {
        self.inner.dimension()
    }

    pub fn track(&self) -> Option<RemoteTrack> {
        self.inner
            .track
            .lock()
            .clone()
            .map(|track| track.try_into().unwrap())
    }

    pub fn mime_type(&self) -> String {
        self.inner.mime_type()
    }

    pub fn muted(&self) -> bool {
        self.inner.muted()
    }

    pub(crate) fn update_track(&self, track: Option<Track>) {
        self.inner.update_track(track);
    }

    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        self.inner.update_info(info);
    }
}
