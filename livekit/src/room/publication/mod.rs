use super::track::{TrackDimension, TrackEvent};
use crate::prelude::*;
use crate::track::Track;
use futures_util::{FutureExt, StreamExt};
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use livekit_protocol::observer::Dispatcher;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;

mod local;
pub use local::*;

mod remote;
pub use remote::*;

#[derive(Debug, Clone)]
pub(crate) enum PublicationEvent {
    UpdateSubscription, // Update subscription needed
}

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
    internal_dispatcher: Dispatcher<PublicationEvent>,
    close_notifier: Arc<Notify>,
    forward_handle: Mutex<Option<JoinHandle<()>>>,
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
            internal_dispatcher: Default::default(),
            close_notifier: Default::default(),
            forward_handle: Default::default(),
        }
    }

    pub async fn update_track(&self, track: Option<Track>) {
        // Make sure to stop the old forwarding task
        if let Some(handle) = self.forward_handle.lock().take() {
            self.close_notifier.notify_waiters();
            handle.await;
        }

        *self.track.lock() = track.clone();

        if let Some(track) = track {
            // Forward track events to the publication
            let pub_dispatcher = self.dispatcher.clone();
            let close_notifier = self.close_notifier.clone();
            tokio::spawn(async move {
                let notified = close_notifier.notified().fuse();
                futures_util::pin_mut!(notified);

                let event_streams = UnboundedReceiverStream::new(track.register_observer());
                let mut forwarding = event_streams.map(Ok).forward(pub_dispatcher);

                futures_util::select! {
                    _ = notified => {},
                    _ = &mut forwarding => {},
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

    pub fn register_observer(&self) -> mpsc::UnboundedReceiver<TrackEvent> {
        self.dispatcher.register()
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

    pub fn is_muted(&self) -> bool {
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
        pub fn is_muted(self: &Self) -> bool;
        pub fn is_remote(self: &Self) -> bool;
    );

    pub fn track(&self) -> Option<Track> {
        match self {
            TrackPublication::Local(p) => p.track().map(Into::into),
            TrackPublication::Remote(p) => p.track().map(Into::into),
        }
    }
}
