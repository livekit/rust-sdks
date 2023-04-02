use super::track::{TrackDimension, TrackEvent};
use crate::prelude::*;
use crate::track::Track;
use futures_util::stream::StreamExt;
use livekit_protocol as proto;
use livekit_utils::enum_dispatch;
use livekit_utils::observer::Dispatcher;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_stream::wrappers::UnboundedReceiverStream;

mod local;
pub use local::*;

mod remote;
pub use remote::*;

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
    dispatcher: Dispatcher<TrackEvent>,
    close_notifier: Arc<Notify>,
}

impl TrackPublicationInner {
    pub fn new(info: proto::TrackInfo, track: Option<Track>) -> Self {
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
            close_notifier: Default::default(),
        }
    }

    pub fn update_track(&self, track: Option<Track>) {
        let mut old_track = self.track.lock();
        *old_track = track.clone();

        self.close_notifier.notify_waiters();

        if let Some(track) = track.as_ref() {
            let track_stream = UnboundedReceiverStream::new(track.register_observer());
            tokio::spawn({
                let dispatcher = self.dispatcher.clone();
                let notifier = self.close_notifier.clone();

                async move {
                    let notified = notifier.notified();
                    futures_util::pin_mut!(notified);
                    futures_util::future::select(
                        track_stream.map(Ok).forward(dispatcher),
                        notified,
                    )
                    .await;
                }
            });
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

    pub fn sid(&self) -> TrackSid {
        self.sid.lock().clone()
    }

    pub fn name(&self) -> String {
        self.name.lock().clone()
    }

    pub fn kind(&self) -> TrackKind {
        self.kind.load(Ordering::SeqCst).try_into().unwrap()
    }

    pub fn source(&self) -> TrackSource {
        self.source.load(Ordering::SeqCst).into()
    }

    pub fn simulcasted(&self) -> bool {
        self.simulcasted.load(Ordering::Relaxed)
    }

    pub fn dimension(&self) -> TrackDimension {
        self.dimension.lock().clone()
    }

    pub fn mime_type(&self) -> String {
        self.mime_type.lock().clone()
    }

    pub fn track(&self) -> Option<Track> {
        self.track.lock().clone()
    }

    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
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
            TrackPublication::Local(p) => p.track().map(Into::into),
            TrackPublication::Remote(p) => p.track().map(Into::into),
        }
    }
}
