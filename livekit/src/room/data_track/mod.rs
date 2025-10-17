use std::marker::PhantomData;
use std::pin::Pin;

use crate::{e2ee::EncryptionType, id::TrackSid};
use futures_util::Stream;
use futures_util::task::{Context, Poll};
use livekit_protocol::{self as proto, DataTrackPublishedResponse};

mod mime;

// TODO: Remove this
pub use mime::Mime;

#[derive(Debug, thiserror::Error)]
pub enum DataTrackError {}

pub type DataTrackResult<T> = Result<T, DataTrackError>;

/// Reserved for future use.
pub mod schema {
    /// Raw bytes.
    pub struct Bytes;
}

#[derive(Clone, Debug)]
pub struct DataTrackOptions<S = schema::Bytes> {
    name: String,
    disable_e2ee: bool,
    mime: Mime,
    _schema: PhantomData<S>,
}

impl<S> DataTrackOptions<S> {
    pub(crate) fn into_add_track_request(
        self,
        use_e2ee: bool
    ) -> proto::AddDataTrackRequest {
        let encryption = if self.disable_e2ee {
            proto::encryption::Type::None
        } else {
            proto::encryption::Type::Gcm
        };
        proto::AddDataTrackRequest {
            name: self.name,
            mime_type: self.mime.to_string(),
            encryption: encryption.into(),
        }
    }
}

impl DataTrackOptions {
    pub fn with_name(name: &str) -> Self {
        Self {
            name: name.to_string(),
            disable_e2ee: false,
            mime: Mime::BINARY,
            _schema: PhantomData,
        }
    }

    pub fn mime(self, mime: Mime) -> Self {
        Self { mime, ..self }
    }

    pub fn disable_e2ee(self, disabled: bool) -> Self {
        Self { disable_e2ee: disabled, ..self }
    }
}

#[derive(Clone, Debug)]
struct DataTrackInfo {
    sid: TrackSid,
    handle: u16,
    name: String,
    mime: Mime,
    encryption: EncryptionType,
}

impl DataTrackInfo {
    pub fn sid(&self) -> &TrackSid {
        &self.sid
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn mime(&self) -> &Mime {
        &self.mime
    }
    pub fn uses_e2ee(&self) -> bool {
        self.encryption != EncryptionType::None
    }
}

struct DataTrackFrame;

/// Marker type indicating a [`DataTrack`] belongs to the local participant.
pub struct Local;

/// Marker type indicating a [`DataTrack`] belongs to a remote participant.
pub struct Remote;

#[derive(Clone, Debug)]
pub struct DataTrack<L, S = schema::Bytes> {
    _location: PhantomData<L>,
    _schema: PhantomData<S>,
    // Need info, way to signal closing by SFU or other
}

impl<L, S> DataTrack<L, S> {
    pub fn info(&self) -> DataTrackInfo {
        todo!()
    }
}

impl DataTrack<Local, schema::Bytes> {
    pub fn publish<F>(&self, frame: impl Into<DataTrackFrame>) -> DataTrackResult<()> {
        todo!()
    }
}

impl DataTrack<Remote, schema::Bytes> {
    pub(crate) fn from_info(info: DataTrackInfo) -> Result<Self, ()> {
        Ok(Self {
            _location: PhantomData,
            _schema: PhantomData,

        })
    }

    pub fn try_with_schema<S>(self) -> Result<DataTrack<Remote, S>, Self> {
        todo!()
    }
}

impl<S> DataTrack<Remote, S> {
    pub fn is_subscribed() -> bool {
        // Subscribed as long as there is at least one subscription
        todo!()
    }

    pub fn subscribe(&self) -> DataTrackResult<DataTrackSubscription<S>> {
        // TODO: send request, create receiver
        todo!()
    }

    pub fn subscribe_with_target(&self, target_fps: u32) -> DataTrackResult<DataTrackSubscription<S>> {
        todo!()
    }
}

pub struct DataTrackSubscription<S> {
    _schema: PhantomData<S>
}

impl<S> Stream for DataTrackSubscription<S> {
    type Item = S;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        todo!();
    }
}