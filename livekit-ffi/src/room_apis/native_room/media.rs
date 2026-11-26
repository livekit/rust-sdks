// Copyright 2026 LiveKit, Inc.
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

//! Thin room-owned media resources. Foreign handles hold only weak room references.

use super::media_frames::{dimensions, invalid, FfiTrack, NativeAudioFrame, NativeVideoFrame};
use super::{FfiPublication, FfiRoom, FfiRoomError, Shared};
use futures_util::StreamExt;
use livekit::{
    options::TrackPublishOptions,
    prelude::*,
    webrtc::{
        audio_source::{native::NativeAudioSource, AudioSourceOptions},
        audio_stream::native::{NativeAudioStream, NativeAudioStreamOptions},
        video_source::{native::NativeVideoSource, RtcVideoSource, VideoResolution},
        video_stream::native::{NativeVideoStream, NativeVideoStreamOptions},
    },
    PlatformAudio, RecordingDeviceId,
};
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

const MAX_RESOURCES: usize = 64;

struct OwnedTrack {
    track: LocalTrack,
    // The platform ADM lives as long as this room-owned device track.
    _device: Option<PlatformAudio>,
}
struct AudioSink {
    sid: String,
    stream: Mutex<NativeAudioStream>,
    cancelled: CancellationToken,
}
struct VideoSink {
    sid: String,
    stream: Mutex<NativeVideoStream>,
    cancelled: CancellationToken,
}

#[derive(Default)]
pub(super) struct Media {
    next_id: u64,
    local: HashMap<u64, Arc<OwnedTrack>>,
    audio: HashMap<u64, Arc<AudioSink>>,
    video: HashMap<u64, Arc<VideoSink>>,
}
impl Media {
    pub(super) fn is_empty(&self) -> bool {
        self.local.is_empty() && self.audio.is_empty() && self.video.is_empty()
    }
    fn id(&mut self) -> Result<u64, FfiRoomError> {
        if self.local.len() + self.audio.len() + self.video.len() >= MAX_RESOURCES {
            return Err(invalid("session media resource limit reached (64)"));
        }
        self.next_id += 1;
        Ok(self.next_id)
    }
    pub(super) fn remove_sinks(&mut self, sid: &str) {
        self.audio.retain(|_, sink| {
            if sink.sid == sid {
                sink.cancelled.cancel();
                false
            } else {
                true
            }
        });
        self.video.retain(|_, sink| {
            if sink.sid == sid {
                sink.cancelled.cancel();
                false
            } else {
                true
            }
        });
    }
    pub(super) fn clear(&mut self) {
        for sink in self.audio.values() {
            sink.cancelled.cancel();
        }
        for sink in self.video.values() {
            sink.cancelled.cancel();
        }
        self.audio.clear();
        self.video.clear();
        self.local.clear();
    }
}

fn session(weak: &Weak<Shared>) -> Result<Arc<Shared>, FfiRoomError> {
    let shared = weak.upgrade().ok_or(FfiRoomError::Closed)?;
    if let Some(error) = &shared.inner.lock().terminal {
        return Err(error.clone());
    }
    Ok(shared)
}
fn local(shared: &Shared, id: u64) -> Result<Arc<OwnedTrack>, FfiRoomError> {
    shared.media.lock().local.get(&id).cloned().ok_or_else(|| invalid("local track was released"))
}
fn publication(room: &Room, identity: &str, sid: &str) -> Result<TrackPublication, FfiRoomError> {
    let sid = sid.to_owned().try_into().map_err(|_| invalid("invalid publication SID"))?;
    if room.local_participant().identity().as_str() == identity {
        room.local_participant()
            .track_publications()
            .get(&sid)
            .cloned()
            .map(TrackPublication::Local)
    } else {
        room.remote_participants()
            .get(&identity.to_owned().into())
            .and_then(|p| p.track_publications().get(&sid).cloned())
            .map(TrackPublication::Remote)
    }
    .ok_or_else(|| invalid("publication is no longer available"))
}

// These existing command/source/sink handles are not substitutes for the core
// Track/Publication objects exported by core_objects.rs. Direct core objects
// cannot carry this room's source ownership, bounded frame queues, explicit
// cancellation, or weak back-reference. IDs/SIDs resolve live room resources;
// after teardown commands fail instead of retaining a room. Frame representation
// restrictions live in media_frames.rs; these handles add no property snapshots.
/// A local audio track owned by the room, including its source. Release explicitly or disconnect.
#[derive(uniffi::Object)]
pub struct NativeLocalAudioTrack {
    session: Weak<Shared>,
    id: u64,
}
/// A local video track owned by the room, including its BGRA source.
#[derive(uniffi::Object)]
pub struct NativeLocalVideoTrack {
    session: Weak<Shared>,
    id: u64,
}
/// A publication reference. Every operation resolves the current core publication by SID.
#[derive(Clone, uniffi::Object)]
pub struct NativePublicationHandle {
    session: Weak<Shared>,
    identity: String,
    sid: String,
}
/// A remote audio track reference; it does not retain the room or a native decoder.
#[derive(uniffi::Object)]
pub struct NativeRemoteAudioTrack {
    publication: NativePublicationHandle,
}
/// A remote video track reference; it does not retain the room or a native decoder.
#[derive(uniffi::Object)]
pub struct NativeRemoteVideoTrack {
    publication: NativePublicationHandle,
}
/// Bounded decoded PCM. The room owns the sink until cancel/drop or disconnect.
#[derive(uniffi::Object)]
pub struct NativeAudioFrames {
    session: Weak<Shared>,
    id: u64,
}
/// Bounded decoded BGRA. The room owns the sink until cancel/drop or disconnect.
#[derive(uniffi::Object)]
pub struct NativeVideoFrames {
    session: Weak<Shared>,
    id: u64,
}

impl FfiRoom {
    fn store_local(&self, track: OwnedTrack) -> Result<u64, FfiRoomError> {
        let inner = self.shared.inner.lock();
        if let Some(error) = &inner.terminal {
            return Err(error.clone());
        }
        let mut media = self.shared.media.lock();
        let id = media.id()?;
        media.local.insert(id, Arc::new(track));
        Ok(id)
    }
}
#[uniffi::export]
impl FfiRoom {
    /// Creates a manual 48 kHz PCM16 source with a bounded 100 ms input queue, mono or stereo.
    /// This injects PCM without requesting microphone capture itself.
    /// Core recording is process-global: an explicitly created microphone track anywhere
    /// in the process can cause physical microphone input to mix into later PCM tracks.
    pub async fn create_audio_track(
        &self,
        name: String,
        channels: u32,
    ) -> Result<Arc<NativeLocalAudioTrack>, FfiRoomError> {
        if !(1..=2).contains(&channels) {
            return Err(invalid("audio channels must be 1 or 2"));
        }
        if let Some(error) = &self.shared.inner.lock().terminal {
            return Err(error.clone());
        }
        let track = super::on_runtime(async move {
            let source =
                NativeAudioSource::new(AudioSourceOptions::default(), 48_000, channels, 100);
            OwnedTrack {
                track: LocalTrack::Audio(LocalAudioTrack::create_audio_track(
                    &name,
                    RtcAudioSource::Native(source),
                )),
                _device: None,
            }
        })
        .await;
        let id = self.store_local(track)?;
        Ok(Arc::new(NativeLocalAudioTrack { session: Arc::downgrade(&self.shared), id }))
    }
    /// Creates a device microphone track using core PlatformAudio. Requires OS microphone permission.
    /// None selects the platform default; a device GUID selects a recording device before capture.
    pub async fn create_microphone_track(
        &self,
        name: String,
        device_id: Option<String>,
    ) -> Result<Arc<NativeLocalAudioTrack>, FfiRoomError> {
        if let Some(error) = &self.shared.inner.lock().terminal {
            return Err(error.clone());
        }
        let track = super::on_runtime(async move {
            let device = PlatformAudio::new().map_err(|e| invalid(&e.to_string()))?;
            if let Some(id) = device_id {
                device
                    .set_recording_device(&RecordingDeviceId::from_unchecked_guid(&id))
                    .map_err(|e| invalid(&e.to_string()))?;
            }
            let track = LocalAudioTrack::create_audio_track(&name, device.rtc_source());
            device.start_recording().map_err(|e| invalid(&e.to_string()))?;
            Ok::<_, FfiRoomError>(OwnedTrack {
                track: LocalTrack::Audio(track),
                _device: Some(device),
            })
        })
        .await?;
        let id = self.store_local(track)?;
        Ok(Arc::new(NativeLocalAudioTrack { session: Arc::downgrade(&self.shared), id }))
    }
    /// Creates a raw video source. Swift capture owns the camera and submits copied BGRA or NV12 frames.
    pub async fn create_video_track(
        &self,
        name: String,
        width: u32,
        height: u32,
        screencast: bool,
    ) -> Result<Arc<NativeLocalVideoTrack>, FfiRoomError> {
        dimensions(width, height)?;
        if let Some(error) = &self.shared.inner.lock().terminal {
            return Err(error.clone());
        }
        let track = super::on_runtime(async move {
            let source = NativeVideoSource::new(VideoResolution { width, height }, screencast);
            OwnedTrack {
                track: LocalTrack::Video(LocalVideoTrack::create_video_track(
                    &name,
                    RtcVideoSource::Native(source),
                )),
                _device: None,
            }
        })
        .await;
        let id = self.store_local(track)?;
        Ok(Arc::new(NativeLocalVideoTrack { session: Arc::downgrade(&self.shared), id }))
    }
    /// Looks up a publication from an event or snapshot without retaining the room in the handle.
    pub fn publication(
        &self,
        identity: String,
        sid: String,
    ) -> Result<Arc<NativePublicationHandle>, FfiRoomError> {
        publication(self.shared.room()?.as_ref(), &identity, &sid)?;
        Ok(Arc::new(NativePublicationHandle {
            session: Arc::downgrade(&self.shared),
            identity,
            sid,
        }))
    }
}

async fn statistics(track: Track) -> Result<Vec<crate::proto::RtcStats>, FfiRoomError> {
    super::on_runtime(async move {
        track
            .get_stats()
            .await
            .map(|stats| stats.into_iter().map(Into::into).collect())
            .map_err(Into::into)
    })
    .await
}

async fn publish(
    weak: &Weak<Shared>,
    id: u64,
    options: TrackPublishOptions,
) -> Result<Arc<NativePublicationHandle>, FfiRoomError> {
    let shared = session(weak)?;
    let room = shared.room()?;
    let owned = local(&shared, id)?;
    let publication = super::on_runtime(async move {
        let publication =
            room.local_participant().publish_track(owned.track.clone(), options).await?;
        Ok::<_, FfiRoomError>((
            room.local_participant().identity().to_string(),
            publication.sid().to_string(),
        ))
    })
    .await?;
    Ok(Arc::new(NativePublicationHandle {
        session: weak.clone(),
        identity: publication.0,
        sid: publication.1,
    }))
}
async fn release(weak: &Weak<Shared>, id: u64) -> Result<(), FfiRoomError> {
    let shared = session(weak)?;
    let owner = shared.clone();
    super::on_runtime(async move {
        let inner = owner.inner.lock();
        let mut media = owner.media.lock();
        if let Some(owned) = media.local.get(&id) {
            if Arc::strong_count(owned) > 1 {
                return Err(invalid("local track has an operation in progress"));
            }
            if let Some(room) = &inner.room {
                if room.local_participant().track_publications().contains_key(&owned.track.sid()) {
                    return Err(invalid("unpublish the track before releasing its source"));
                }
            }
        }
        let owned = media.local.remove(&id);
        drop(media);
        drop(inner);
        drop(owned);
        Ok(())
    })
    .await
}
#[uniffi::export]
impl NativeLocalAudioTrack {
    /// Samples core track statistics. Numeric values, microsecond timestamps and RTP groups are preserved.
    /// The host owns periodic sampling and previous-sample calculations.
    pub async fn get_statistics(&self) -> Result<Vec<crate::proto::RtcStats>, FfiRoomError> {
        let shared = session(&self.session)?;
        let owned = local(&shared, self.id)?;
        statistics(owned.track.clone().into()).await
    }
    /// Returns the current core track values.
    pub fn info(&self) -> Result<FfiTrack, FfiRoomError> {
        Ok(Track::from(local(session(&self.session)?.as_ref(), self.id)?.track.clone()))
    }
    /// Mutes/unmutes the core local track, including before publication.
    pub async fn set_muted(&self, muted: bool) -> Result<(), FfiRoomError> {
        let shared = session(&self.session)?;
        let owned = local(&shared, self.id)?;
        super::on_runtime(async move {
            if muted {
                owned.track.mute();
            } else {
                owned.track.unmute();
            }
        })
        .await;
        Ok(())
    }
    /// Publishes this track using the connected room.
    pub async fn publish(
        &self,
        options: TrackPublishOptions,
    ) -> Result<Arc<NativePublicationHandle>, FfiRoomError> {
        publish(&self.session, self.id, options).await
    }
    /// Queues PCM on a manual source. Device microphone sources reject manual injection.
    pub async fn capture_frame(&self, frame: NativeAudioFrame) -> Result<(), FfiRoomError> {
        frame.validate()?;
        let shared = session(&self.session)?;
        let LocalTrack::Audio(track) = local(&shared, self.id)?.track.clone() else {
            return Err(invalid("not an audio track"));
        };
        let RtcAudioSource::Native(source) = track.rtc_source() else {
            return Err(invalid("device microphone does not accept PCM injection"));
        };
        if frame.num_channels != source.num_channels() {
            return Err(invalid("PCM channel count differs from source"));
        }
        super::on_runtime(async move {
            source.capture_frame(&frame.into_core()).await.map_err(|e| invalid(&e.to_string()))
        })
        .await
    }
    /// Discards the queued PCM on a manual source.
    pub fn clear_buffer(&self) -> Result<(), FfiRoomError> {
        let shared = session(&self.session)?;
        if let LocalTrack::Audio(track) = local(&shared, self.id)?.track.clone() {
            if let RtcAudioSource::Native(source) = track.rtc_source() {
                source.clear_buffer();
            }
        }
        Ok(())
    }
    /// Releases an unpublished source. Published tracks must be unpublished first.
    pub async fn release(&self) -> Result<(), FfiRoomError> {
        release(&self.session, self.id).await
    }
}
#[uniffi::export]
impl NativeLocalVideoTrack {
    /// Samples core track statistics. Numeric values, microsecond timestamps and RTP groups are preserved.
    /// The host owns periodic sampling and previous-sample calculations.
    pub async fn get_statistics(&self) -> Result<Vec<crate::proto::RtcStats>, FfiRoomError> {
        let shared = session(&self.session)?;
        let owned = local(&shared, self.id)?;
        statistics(owned.track.clone().into()).await
    }
    /// Returns the current core track values.
    pub fn info(&self) -> Result<FfiTrack, FfiRoomError> {
        Ok(Track::from(local(session(&self.session)?.as_ref(), self.id)?.track.clone()))
    }
    /// Mutes/unmutes the core local track, including before publication.
    pub async fn set_muted(&self, muted: bool) -> Result<(), FfiRoomError> {
        let shared = session(&self.session)?;
        let owned = local(&shared, self.id)?;
        super::on_runtime(async move {
            if muted {
                owned.track.mute();
            } else {
                owned.track.unmute();
            }
        })
        .await;
        Ok(())
    }
    /// Publishes this track using the connected room.
    pub async fn publish(
        &self,
        options: TrackPublishOptions,
    ) -> Result<Arc<NativePublicationHandle>, FfiRoomError> {
        publish(&self.session, self.id, options).await
    }
    /// Converts copied BGRA/NV12 and offers it to the core; false means frame adaptation dropped it.
    pub async fn capture_frame(&self, frame: NativeVideoFrame) -> Result<bool, FfiRoomError> {
        let shared = session(&self.session)?;
        let LocalTrack::Video(track) = local(&shared, self.id)?.track.clone() else {
            return Err(invalid("not a video track"));
        };
        let RtcVideoSource::Native(source) = track.rtc_source() else {
            return Err(invalid("unsupported video source"));
        };
        super::on_runtime(async move {
            Ok::<_, FfiRoomError>(source.capture_frame(&frame.into_core()?))
        })
        .await
    }
    /// Releases an unpublished source. Published tracks must be unpublished first.
    pub async fn release(&self) -> Result<(), FfiRoomError> {
        release(&self.session, self.id).await
    }
}

impl NativePublicationHandle {
    fn current(&self) -> Result<(Arc<Shared>, TrackPublication), FfiRoomError> {
        let shared = session(&self.session)?;
        let p = publication(shared.room()?.as_ref(), &self.identity, &self.sid)?;
        Ok((shared, p))
    }
    fn remote(&self) -> Result<(Arc<Shared>, RemoteTrackPublication), FfiRoomError> {
        let (shared, publication) = self.current()?;
        let TrackPublication::Remote(p) = publication else {
            return Err(invalid("operation requires a remote publication"));
        };
        Ok((shared, p))
    }
}
#[uniffi::export]
impl NativePublicationHandle {
    /// Samples this publication's current local or subscribed remote track.
    pub async fn get_statistics(&self) -> Result<Vec<crate::proto::RtcStats>, FfiRoomError> {
        let (_shared, publication) = self.current()?;
        let track: Track = match publication {
            TrackPublication::Local(p) => {
                p.track().ok_or_else(|| invalid("publication has no track"))?.into()
            }
            TrackPublication::Remote(p) => {
                p.track().ok_or_else(|| invalid("track is not subscribed"))?.into()
            }
        };
        statistics(track).await
    }
    /// Returns a value snapshot of the current core publication.
    pub fn info(&self) -> Result<FfiPublication, FfiRoomError> {
        let (shared, publication) = self.current()?;
        let room = shared.room()?;
        let local = room.local_participant();
        let participant = if local.identity().as_str() == self.identity {
            Participant::Local(local)
        } else {
            Participant::Remote(
                room.remote_participants()
                    .get(&self.identity.clone().into())
                    .cloned()
                    .ok_or_else(|| invalid("participant not found"))?,
            )
        };
        Ok(FfiPublication::new(&participant, &publication))
    }
    /// Mutes or unmutes a local publication through core track controls.
    pub async fn set_muted(&self, muted: bool) -> Result<(), FfiRoomError> {
        let (_shared, p) = self.current()?;
        let TrackPublication::Local(p) = p else {
            return Err(invalid("only local publications can be muted"));
        };
        super::on_runtime(async move {
            if muted {
                p.mute();
            } else {
                p.unmute();
            }
        })
        .await;
        Ok(())
    }
    /// Unpublishes a local track. The source remains available until release or room teardown.
    pub async fn unpublish(&self) -> Result<(), FfiRoomError> {
        let (shared, p) = self.current()?;
        if !matches!(p, TrackPublication::Local(_)) {
            return Err(invalid("only local publications can be unpublished"));
        }
        let room = shared.room()?;
        super::on_runtime(async move {
            room.local_participant().unpublish_track(&p.sid()).await.map(|_| ()).map_err(Into::into)
        })
        .await
    }
    /// Sets core subscription desire, not an SFU acknowledgment. Core emits unsubscribe locally;
    /// an immediate false->true flip may coalesce signaling and never emit a new subscribed event.
    pub async fn set_subscribed(&self, subscribed: bool) -> Result<(), FfiRoomError> {
        let (_shared, p) = self.remote()?;
        super::on_runtime(async move {
            p.set_subscribed(subscribed);
        })
        .await;
        Ok(())
    }
    /// Enables/disables receiving a subscribed remote publication.
    pub async fn set_enabled(&self, enabled: bool) -> Result<(), FfiRoomError> {
        let (_shared, p) = self.remote()?;
        super::on_runtime(async move {
            p.set_enabled(enabled);
        })
        .await;
        Ok(())
    }
    /// Requests the highest acceptable simulcast layer. Core ignores this for non-simulcast tracks.
    pub async fn set_video_quality(
        &self,
        quality: super::VideoQuality,
    ) -> Result<(), FfiRoomError> {
        let (_shared, p) = self.remote()?;
        if p.kind() != TrackKind::Video {
            return Err(invalid("not a video publication"));
        }
        super::on_runtime(async move {
            p.set_video_quality(quality);
        })
        .await;
        Ok(())
    }
    /// Requests remote video dimensions. Core has no preferred-FPS control.
    pub async fn set_video_dimensions(&self, width: u32, height: u32) -> Result<(), FfiRoomError> {
        dimensions(width, height)?;
        let (_shared, p) = self.remote()?;
        if p.kind() != TrackKind::Video {
            return Err(invalid("not a video publication"));
        }
        super::on_runtime(async move {
            p.update_video_dimensions(TrackDimension(width, height));
        })
        .await;
        Ok(())
    }
    /// Returns a typed audio handle if this remote publication is currently subscribed to audio.
    pub fn remote_audio_track(&self) -> Result<Option<Arc<NativeRemoteAudioTrack>>, FfiRoomError> {
        let (_, p) = self.remote()?;
        Ok(matches!(p.track(), Some(RemoteTrack::Audio(_)))
            .then(|| Arc::new(NativeRemoteAudioTrack { publication: self.clone() })))
    }
    /// Returns a typed video handle if this remote publication is currently subscribed to video.
    pub fn remote_video_track(&self) -> Result<Option<Arc<NativeRemoteVideoTrack>>, FfiRoomError> {
        let (_, p) = self.remote()?;
        Ok(matches!(p.track(), Some(RemoteTrack::Video(_)))
            .then(|| Arc::new(NativeRemoteVideoTrack { publication: self.clone() })))
    }
}

#[uniffi::export]
impl NativeRemoteAudioTrack {
    /// Samples the subscribed core track without a native timer or cached statistics.
    pub async fn get_statistics(&self) -> Result<Vec<crate::proto::RtcStats>, FfiRoomError> {
        let (_shared, publication) = self.publication.remote()?;
        let track = publication.track().ok_or_else(|| invalid("track is not subscribed"))?;
        statistics(track.into()).await
    }
    /// Returns values for the currently subscribed core track.
    pub fn info(&self) -> Result<FfiTrack, FfiRoomError> {
        let (_, p) = self.publication.remote()?;
        let track = p.track().ok_or_else(|| invalid("track is not subscribed"))?;
        Ok(Track::from(track))
    }
    /// Attaches a bounded decoded 48 kHz PCM16 sink (1..64 frames, mono/stereo).
    pub async fn frames(
        &self,
        channels: u32,
        capacity: u32,
    ) -> Result<Arc<NativeAudioFrames>, FfiRoomError> {
        if !(1..=2).contains(&channels) || !(1..=64).contains(&capacity) {
            return Err(invalid("channels must be 1..2 and capacity 1..64"));
        }
        let (shared, p) = self.publication.remote()?;
        let Some(RemoteTrack::Audio(track)) = p.track() else {
            return Err(invalid("audio track is not subscribed"));
        };
        let sink = super::on_runtime(async move {
            NativeAudioStream::with_options(
                track.rtc_track(),
                48_000,
                channels as i32,
                NativeAudioStreamOptions { queue_size_frames: Some(capacity as usize) },
            )
        })
        .await;
        let inner = shared.inner.lock();
        if let Some(error) = &inner.terminal {
            return Err(error.clone());
        }
        if p.track().is_none() {
            return Err(invalid("track was unsubscribed"));
        }
        let mut media = shared.media.lock();
        let id = media.id()?;
        media.audio.insert(
            id,
            Arc::new(AudioSink {
                sid: p.sid().to_string(),
                stream: Mutex::new(sink),
                cancelled: CancellationToken::new(),
            }),
        );
        Ok(Arc::new(NativeAudioFrames { session: self.publication.session.clone(), id }))
    }
}
#[uniffi::export]
impl NativeRemoteVideoTrack {
    /// Samples the subscribed core track without a native timer or cached statistics.
    pub async fn get_statistics(&self) -> Result<Vec<crate::proto::RtcStats>, FfiRoomError> {
        let (_shared, publication) = self.publication.remote()?;
        let track = publication.track().ok_or_else(|| invalid("track is not subscribed"))?;
        statistics(track.into()).await
    }
    /// Returns values for the currently subscribed core track.
    pub fn info(&self) -> Result<FfiTrack, FfiRoomError> {
        let (_, p) = self.publication.remote()?;
        let track = p.track().ok_or_else(|| invalid("track is not subscribed"))?;
        Ok(Track::from(track))
    }
    /// Attaches a bounded decoded video sink (1..8 frames). Slow consumers lose frames.
    pub async fn frames(&self, capacity: u32) -> Result<Arc<NativeVideoFrames>, FfiRoomError> {
        if !(1..=8).contains(&capacity) {
            return Err(invalid("video capacity must be 1..8"));
        }
        let (shared, p) = self.publication.remote()?;
        let Some(RemoteTrack::Video(track)) = p.track() else {
            return Err(invalid("video track is not subscribed"));
        };
        let sink = super::on_runtime(async move {
            NativeVideoStream::with_options(
                track.rtc_track(),
                NativeVideoStreamOptions { queue_size_frames: Some(capacity as usize) },
            )
        })
        .await;
        let inner = shared.inner.lock();
        if let Some(error) = &inner.terminal {
            return Err(error.clone());
        }
        if p.track().is_none() {
            return Err(invalid("track was unsubscribed"));
        }
        let mut media = shared.media.lock();
        let id = media.id()?;
        media.video.insert(
            id,
            Arc::new(VideoSink {
                sid: p.sid().to_string(),
                stream: Mutex::new(sink),
                cancelled: CancellationToken::new(),
            }),
        );
        Ok(Arc::new(NativeVideoFrames { session: self.publication.session.clone(), id }))
    }
}
#[uniffi::export]
impl NativeAudioFrames {
    /// Waits for decoded PCM. Cancel returns None; room teardown returns a room error.
    pub async fn next(&self) -> Result<Option<NativeAudioFrame>, FfiRoomError> {
        let shared = session(&self.session)?;
        let Some(sink) = shared.media.lock().audio.get(&self.id).cloned() else {
            return Ok(None);
        };
        Ok(super::on_runtime(async move {
            tokio::select! { biased;
                _ = sink.cancelled.cancelled() => None,
                frame = async { sink.stream.lock().await.next().await } => frame.map(Into::into),
            }
        })
        .await)
    }
    /// Detaches the sink and wakes pending reads. Defer this call from a Swift onCancel handler.
    pub fn cancel(&self) {
        if let Some(shared) = self.session.upgrade() {
            let sink = shared.media.lock().audio.remove(&self.id);
            if let Some(sink) = sink {
                sink.cancelled.cancel();
            }
        }
    }
}
impl Drop for NativeAudioFrames {
    fn drop(&mut self) {
        self.cancel();
    }
}
#[uniffi::export]
impl NativeVideoFrames {
    /// Waits for a decoded frame converted to copied BGRA bytes.
    pub async fn next(&self) -> Result<Option<NativeVideoFrame>, FfiRoomError> {
        let shared = session(&self.session)?;
        let Some(sink) = shared.media.lock().video.get(&self.id).cloned() else {
            return Ok(None);
        };
        super::on_runtime(async move {
            tokio::select! { biased;
                _ = sink.cancelled.cancelled() => Ok(None),
                frame = async { sink.stream.lock().await.next().await } => frame.map(NativeVideoFrame::from_core).transpose(),
            }
        }).await
    }
    /// Detaches the sink and wakes pending reads. Defer this call from a Swift onCancel handler.
    pub fn cancel(&self) {
        if let Some(shared) = self.session.upgrade() {
            let sink = shared.media.lock().video.remove(&self.id);
            if let Some(sink) = sink {
                sink.cancelled.cancel();
            }
        }
    }
}
impl Drop for NativeVideoFrames {
    fn drop(&mut self) {
        self.cancel();
    }
}
