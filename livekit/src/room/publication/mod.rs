use crate::prelude::*;
use crate::proto;
use livekit_utils::enum_dispatch;
use livekit_utils::observer::Dispatcher;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

use super::track::{TrackDimension, TrackEvent};

pub(crate) trait TrackPublicationInternalTrait {
    fn update_track(&self, track: Option<TrackHandle>);
    fn update_info(&self, info: proto::TrackInfo);
}

pub trait TrackPublicationTrait {
    fn name(&self) -> String;
    fn sid(&self) -> TrackSid;
    fn kind(&self) -> TrackKind;
    fn source(&self) -> TrackSource;
    fn simulcasted(&self) -> bool;
    fn dimension(&self) -> TrackDimension;
    fn mime_type(&self) -> String;
    fn muted(&self) -> bool;
}

#[derive(Debug)]
pub(super) struct TrackPublicationShared {
    pub(super) track: Mutex<Option<TrackHandle>>,
    pub(super) name: Mutex<String>,
    pub(super) sid: Mutex<TrackSid>,
    pub(super) kind: AtomicU8,   // Casted to TrackKind
    pub(super) source: AtomicU8, // Casted to TrackSource
    pub(super) simulcasted: AtomicBool,
    pub(super) dimension: Mutex<TrackDimension>,
    pub(super) mime_type: Mutex<String>,
    pub(super) muted: AtomicBool,
    pub(super) participant: ParticipantSid,
    pub(super) dispatcher: Mutex<Dispatcher<TrackEvent>>,
    pub(super) close_sender: Mutex<Option<oneshot::Sender<()>>>,
}

impl TrackPublicationShared {
    pub fn new(
        info: proto::TrackInfo,
        participant: ParticipantSid,
        track: Option<TrackHandle>,
    ) -> Arc<Self> {
        Arc::new(Self {
            track: Mutex::new(track),
            name: Mutex::new(info.name),
            sid: Mutex::new(info.sid.into()),
            kind: AtomicU8::new(
                TrackKind::from(proto::TrackType::from_i32(info.r#type).unwrap()) as u8,
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
        })
    }

    pub fn update_track(self: &Arc<Self>, track: Option<TrackHandle>) {
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

    /// Task used to forward TrackHandle's events to the TrackPublications's dispatcher
    async fn publication_task(
        self: Arc<Self>,
        mut close_receiver: oneshot::Receiver<()>,
        mut track_receiver: mpsc::UnboundedReceiver<TrackEvent>,
    ) {
        loop {
            tokio::select! {
                Some(event) = track_receiver.recv() => {
                    self.dispatcher.lock().dispatch(&event);
                }
                _ = &mut close_receiver => {
                    break;
                }
            }
        }
    }

    pub fn update_info(&self, info: proto::TrackInfo) {
        *self.name.lock() = info.name;
        *self.sid.lock() = info.sid.into();
        *self.dimension.lock() = TrackDimension(info.width, info.height);
        *self.mime_type.lock() = info.mime_type;
        self.kind.store(
            TrackKind::from(proto::TrackType::from_i32(info.r#type).unwrap()) as u8,
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
}

impl Drop for TrackPublicationShared {
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
    pub fn track(&self) -> Option<TrackHandle> {
        // Not calling Local/Remote function here, we don't need "cast"
        match self {
            TrackPublication::Local(p) => p.shared.track.lock().clone(),
            TrackPublication::Remote(p) => p.shared.track.lock().clone(),
        }
    }
}

impl TrackPublicationInternalTrait for TrackPublication {
    enum_dispatch!(
        [Local, Remote]
        fnc!(update_track, &Self, [track: Option<TrackHandle>], ());
        fnc!(update_info, &Self, [info: proto::TrackInfo], ());
    );
}

impl TrackPublicationTrait for TrackPublication {
    enum_dispatch!(
        [Local, Remote]
        fnc!(sid, &Self, [], TrackSid);
        fnc!(name, &Self, [], String);
        fnc!(kind, &Self, [], TrackKind);
        fnc!(source, &Self, [], TrackSource);
        fnc!(simulcasted, &Self, [], bool);
        fnc!(dimension, &Self, [], TrackDimension);
        fnc!(mime_type, &Self, [], String);
        fnc!(muted, &Self, [], bool);
    );
}

macro_rules! impl_publication_trait {
    ($x:ident) => {
        impl TrackPublicationTrait for $x {
            fn name(&self) -> String {
                self.shared.name.lock().clone()
            }

            fn sid(&self) -> TrackSid {
                self.shared.sid.lock().clone()
            }

            fn kind(&self) -> TrackKind {
                self.shared.kind.load(Ordering::SeqCst).into()
            }

            fn source(&self) -> TrackSource {
                self.shared.source.load(Ordering::SeqCst).into()
            }

            fn simulcasted(&self) -> bool {
                self.shared.simulcasted.load(Ordering::SeqCst)
            }

            fn dimension(&self) -> TrackDimension {
                self.shared.dimension.lock().clone()
            }

            fn mime_type(&self) -> String {
                self.shared.mime_type.lock().clone()
            }

            fn muted(&self) -> bool {
                self.shared.muted.load(Ordering::SeqCst)
            }
        }
    };
}

#[derive(Clone, Debug)]
pub struct LocalTrackPublication {
    shared: Arc<TrackPublicationShared>,
}

impl LocalTrackPublication {
    pub fn track(&self) -> Option<LocalTrackHandle> {
        self.shared
            .track
            .lock()
            .clone()
            .map(|local_track| local_track.try_into().unwrap())
    }
}

impl TrackPublicationInternalTrait for LocalTrackPublication {
    fn update_track(&self, track: Option<TrackHandle>) {
        self.shared.update_track(track);
    }

    fn update_info(&self, info: proto::TrackInfo) {
        self.shared.update_info(info);
    }
}

#[derive(Clone, Debug)]
pub struct RemoteTrackPublication {
    shared: Arc<TrackPublicationShared>,
}

impl RemoteTrackPublication {
    pub fn new(
        info: proto::TrackInfo,
        participant: ParticipantSid,
        track: Option<TrackHandle>,
    ) -> Self {
        Self {
            shared: TrackPublicationShared::new(info, participant, track),
        }
    }

    pub fn track(&self) -> Option<RemoteTrackHandle> {
        self.shared
            .track
            .lock()
            .clone()
            .map(|track| track.try_into().unwrap())
    }
}

impl TrackPublicationInternalTrait for RemoteTrackPublication {
    fn update_track(&self, track: Option<TrackHandle>) {
        self.shared.update_track(track);
    }

    fn update_info(&self, info: proto::TrackInfo) {
        self.shared.update_info(info);
    }
}

impl_publication_trait!(LocalTrackPublication);
impl_publication_trait!(RemoteTrackPublication);
