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
    fmt::Debug,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use libwebrtc::{
    native::packet_trailer::{
        self, PacketTrailerHandler, PublishTimingObserver as RtcPublishTimingObserver,
    },
    prelude::*,
    stats::RtcStats,
};
use livekit_protocol as proto;
use parking_lot::Mutex;
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, Stream};

use super::{TrackInner, VideoQuality};
use crate::{prelude::*, rtc_engine::lk_runtime::LkRuntime};

pub use libwebrtc::native::packet_trailer::{PublishTimingEvent, PublishTimingStage};

const PUBLISH_TIMING_BUFFER: usize = 256;
const HIGH_RID: &str = "f";
const MEDIUM_RID: &str = "h";
const LOW_RID: &str = "q";

#[derive(Clone)]
pub struct LocalVideoTrack {
    inner: Arc<TrackInner>,
    source: RtcVideoSource,
    baseline_encodings: Arc<Mutex<Option<Vec<RtpEncodingParameters>>>>,
    packet_trailer_handler: Arc<Mutex<Option<PacketTrailerHandler>>>,
    publish_timing_tx: Arc<Mutex<Option<broadcast::Sender<PublishTimingEvent>>>>,
}

/// Runtime encoding limits for a published local video track.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VideoEncodingLimits {
    /// Maximum encoded bitrate in bits per second.
    pub max_bitrate: Option<u64>,
    /// Maximum encoded frame rate in frames per second.
    pub max_framerate: Option<f64>,
    /// Encoded resolution downscale factor.
    pub scale_resolution_down_by: Option<f64>,
}

impl Debug for LocalVideoTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalVideoTrack")
            .field("sid", &self.sid())
            .field("name", &self.name())
            .field("source", &self.source())
            .finish()
    }
}

/// A stream of native local video publish-pipeline timing events.
pub struct PublishTimingEventStream {
    inner: BroadcastStream<PublishTimingEvent>,
}

impl Stream for PublishTimingEventStream {
    type Item = PublishTimingEvent;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(event))) => return Poll::Ready(Some(event)),
                Poll::Ready(Some(Err(_))) => continue,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl LocalVideoTrack {
    pub fn new(name: String, rtc_track: RtcVideoTrack, source: RtcVideoSource) -> Self {
        Self {
            inner: Arc::new(super::new_inner(
                "TR_unknown".to_owned().try_into().unwrap(),
                name,
                TrackKind::Video,
                MediaStreamTrack::Video(rtc_track),
            )),
            source,
            baseline_encodings: Arc::new(Mutex::new(None)),
            packet_trailer_handler: Arc::new(Mutex::new(None)),
            publish_timing_tx: Arc::new(Mutex::new(None)),
        }
    }

    pub fn create_video_track(name: &str, source: RtcVideoSource) -> LocalVideoTrack {
        let rtc_track = match source.clone() {
            #[cfg(not(target_arch = "wasm32"))]
            RtcVideoSource::Native(native_source) => {
                use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
                LkRuntime::instance()
                    .pc_factory()
                    .create_video_track(&libwebrtc::native::create_random_uuid(), native_source)
            }
            _ => panic!("unsupported video source"),
        };

        Self::new(name.to_string(), rtc_track, source)
    }

    pub fn sid(&self) -> TrackSid {
        self.inner.info.read().sid.clone()
    }

    pub fn name(&self) -> String {
        self.inner.info.read().name.clone()
    }

    pub fn kind(&self) -> TrackKind {
        self.inner.info.read().kind
    }

    pub fn source(&self) -> TrackSource {
        self.inner.info.read().source
    }

    pub fn stream_state(&self) -> StreamState {
        self.inner.info.read().stream_state
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.rtc_track.enabled()
    }

    pub fn enable(&self) {
        self.inner.rtc_track.set_enabled(true);
    }

    pub fn disable(&self) {
        self.inner.rtc_track.set_enabled(false);
    }

    pub fn is_muted(&self) -> bool {
        self.inner.info.read().muted
    }

    pub fn mute(&self) {
        super::set_muted(&self.inner, &Track::LocalVideo(self.clone()), true);
    }

    pub fn unmute(&self) {
        super::set_muted(&self.inner, &Track::LocalVideo(self.clone()), false);
    }

    pub fn rtc_track(&self) -> RtcVideoTrack {
        if let MediaStreamTrack::Video(video) = self.inner.rtc_track.clone() {
            return video;
        }
        unreachable!();
    }

    pub fn is_remote(&self) -> bool {
        false
    }

    pub fn rtc_source(&self) -> RtcVideoSource {
        self.source.clone()
    }

    /// Sets runtime encoding limits for this published video track.
    ///
    /// Pass `None` for an individual field to restore the original publish-time
    /// encoding value. The track must be published before this method can
    /// update sender parameters. When the track is
    /// simulcasted, `Some` values target the high layer and lower layers keep
    /// the same ratios as the original publish-time encoding ladder.
    pub(crate) fn set_encoding_limits(&self, limits: VideoEncodingLimits) -> RoomResult<()> {
        log::debug!("applying track-level local video encoding limits: {limits:?}");
        self.update_encoding_parameters(|encodings, baseline| {
            apply_track_encoding_limits(encodings, baseline, limits)
        })
    }

    fn update_encoding_parameters(
        &self,
        update: impl FnOnce(&mut [RtpEncodingParameters], &[RtpEncodingParameters]) -> RoomResult<()>,
    ) -> RoomResult<()> {
        let Some(transceiver) = self.transceiver() else {
            return Err(RoomError::Rtc(RtcError {
                error_type: RtcErrorType::InvalidState,
                message: "track is not published".into(),
            }));
        };

        let sender = transceiver.sender();
        let mut parameters = sender.parameters();
        let baseline = {
            let mut baseline_encodings = self.baseline_encodings.lock();
            baseline_encodings.get_or_insert_with(|| parameters.encodings.clone()).clone()
        };
        let before = parameters.encodings.clone();

        update(&mut parameters.encodings, &baseline)?;
        log::debug!(
            "local video sender encoding update: baseline=[{}], before=[{}], after=[{}]",
            format_encoding_parameters(&baseline),
            format_encoding_parameters(&before),
            format_encoding_parameters(&parameters.encodings)
        );
        sender.set_parameters(parameters)?;

        Ok(())
    }

    /// Returns a stream of native local video publish-pipeline timing events.
    ///
    /// Multiple concurrent subscriptions are supported; each call returns an
    /// independent stream that begins with the next event observed after this
    /// call. Slow consumers will silently drop intermediate events when the
    /// internal buffer fills.
    ///
    /// The underlying transformer is allocated lazily on first call. If invoked
    /// before the track is published, instrumentation is enabled at publish time.
    pub fn publish_timing_events(&self) -> PublishTimingEventStream {
        let tx = {
            let mut publish_timing_tx = self.publish_timing_tx.lock();
            if let Some(tx) = publish_timing_tx.as_ref() {
                tx.clone()
            } else {
                let (tx, _) = broadcast::channel(PUBLISH_TIMING_BUFFER);
                *publish_timing_tx = Some(tx.clone());
                tx
            }
        };

        let handler = self.ensure_publish_timing_handler();
        if let Some(handler) = handler {
            self.apply_publish_timing_observer(&handler);
        }

        PublishTimingEventStream { inner: BroadcastStream::new(tx.subscribe()) }
    }

    /// Returns the packet trailer handler associated with this track, if any.
    /// When present on the sender side, callers can store per-frame user
    /// timestamps which will be embedded into encoded frames.
    pub(crate) fn packet_trailer_handler(&self) -> Option<PacketTrailerHandler> {
        self.packet_trailer_handler.lock().clone()
    }

    pub(crate) fn has_publish_timing_subscribers(&self) -> bool {
        self.publish_timing_tx.lock().is_some()
    }

    /// Internal: set the packet trailer handler used for this track.
    pub(crate) fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
        self.apply_publish_timing_observer(&handler);
        *self.packet_trailer_handler.lock() = Some(handler);
    }

    fn ensure_publish_timing_handler(&self) -> Option<PacketTrailerHandler> {
        if let Some(handler) = self.packet_trailer_handler.lock().clone() {
            return Some(handler);
        }

        let transceiver = self.transceiver()?;
        let handler = packet_trailer::create_sender_handler(
            LkRuntime::instance().pc_factory(),
            &transceiver.sender(),
        );
        handler.set_enabled(false);
        self.set_packet_trailer_handler(handler.clone());

        #[cfg(not(target_arch = "wasm32"))]
        if let RtcVideoSource::Native(ref native_source) = self.rtc_source() {
            native_source.set_packet_trailer_handler(handler.clone());
        }

        Some(handler)
    }

    fn apply_publish_timing_observer(&self, handler: &PacketTrailerHandler) {
        let tx = self.publish_timing_tx.lock().clone();
        let observer = tx.map(|tx| {
            Arc::new(move |event: PublishTimingEvent| {
                let _ = tx.send(event);
            }) as RtcPublishTimingObserver
        });
        handler.set_publish_timing_observer(observer);
    }

    pub async fn get_stats(&self) -> RoomResult<Vec<RtcStats>> {
        super::local_track::get_stats(&self.inner).await
    }

    pub(crate) fn on_muted(&self, f: impl Fn(Track) + Send + 'static) {
        self.inner.events.lock().muted.replace(Box::new(f));
    }

    pub(crate) fn on_unmuted(&self, f: impl Fn(Track) + Send + 'static) {
        self.inner.events.lock().unmuted.replace(Box::new(f));
    }

    pub(crate) fn transceiver(&self) -> Option<RtpTransceiver> {
        self.inner.info.read().transceiver.clone()
    }

    pub(crate) fn set_transceiver(&self, transceiver: Option<RtpTransceiver>) {
        let baseline = transceiver.as_ref().map(|transceiver| {
            let sender = transceiver.sender();
            sender.parameters().encodings
        });
        log::debug!(
            "local video sender baseline encodings: [{}]",
            baseline
                .as_deref()
                .map(format_encoding_parameters)
                .unwrap_or_else(|| "none".to_string())
        );
        *self.baseline_encodings.lock() = baseline;
        self.inner.info.write().transceiver = transceiver;
    }

    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        super::update_info(&self.inner, &Track::LocalVideo(self.clone()), info);
    }
}

fn apply_track_encoding_limits(
    encodings: &mut [RtpEncodingParameters],
    baseline: &[RtpEncodingParameters],
    limits: VideoEncodingLimits,
) -> RoomResult<()> {
    validate_encoding_baseline(encodings, baseline)?;

    if encodings.len() == 1 {
        encodings[0] = exact_encoding_limits(&encodings[0], &baseline[0], limits);
        return Ok(());
    }

    let high_baseline = encoding_for_quality(baseline, VideoQuality::High)?;
    let mut updated = Vec::with_capacity(encodings.len());

    for encoding in encodings.iter() {
        let quality = quality_for_rid(&encoding.rid).ok_or_else(|| {
            invalid_state(format!("unsupported simulcast RID '{}'", encoding.rid))
        })?;
        let encoding_baseline = encoding_for_quality(baseline, quality)?;
        updated.push(scaled_encoding_limits(encoding, encoding_baseline, high_baseline, limits)?);
    }

    encodings.clone_from_slice(&updated);
    Ok(())
}

fn validate_encoding_baseline(
    encodings: &[RtpEncodingParameters],
    baseline: &[RtpEncodingParameters],
) -> RoomResult<()> {
    if encodings.is_empty() {
        return Err(invalid_state("track has no RTP encodings"));
    }
    if encodings.len() != baseline.len() {
        return Err(invalid_state(format!(
            "sender encoding count changed from {} to {}",
            baseline.len(),
            encodings.len()
        )));
    }
    Ok(())
}

fn scaled_encoding_limits(
    encoding: &RtpEncodingParameters,
    baseline: &RtpEncodingParameters,
    high_baseline: &RtpEncodingParameters,
    limits: VideoEncodingLimits,
) -> RoomResult<RtpEncodingParameters> {
    let mut updated = encoding.clone();
    updated.max_bitrate = match limits.max_bitrate {
        Some(max_bitrate) => Some(scale_u64(
            max_bitrate,
            required_u64(baseline.max_bitrate, "baseline max_bitrate")?,
            required_u64(high_baseline.max_bitrate, "high baseline max_bitrate")?,
        )?),
        None => baseline.max_bitrate,
    };
    updated.max_framerate = match limits.max_framerate {
        Some(max_framerate) => Some(scale_f64(
            max_framerate,
            required_f64(baseline.max_framerate, "baseline max_framerate")?,
            required_f64(high_baseline.max_framerate, "high baseline max_framerate")?,
        )?),
        None => baseline.max_framerate,
    };
    updated.scale_resolution_down_by = match limits.scale_resolution_down_by {
        Some(scale_resolution_down_by) => Some(scale_f64(
            scale_resolution_down_by,
            required_f64(baseline.scale_resolution_down_by, "baseline scale_resolution_down_by")?,
            required_f64(
                high_baseline.scale_resolution_down_by,
                "high baseline scale_resolution_down_by",
            )?,
        )?),
        None => baseline.scale_resolution_down_by,
    };

    Ok(updated)
}

fn exact_encoding_limits(
    encoding: &RtpEncodingParameters,
    baseline: &RtpEncodingParameters,
    limits: VideoEncodingLimits,
) -> RtpEncodingParameters {
    let mut updated = encoding.clone();
    updated.max_bitrate = limits.max_bitrate.or(baseline.max_bitrate);
    updated.max_framerate = limits.max_framerate.or(baseline.max_framerate);
    updated.scale_resolution_down_by =
        limits.scale_resolution_down_by.or(baseline.scale_resolution_down_by);
    updated
}

fn required_u64(value: Option<u64>, field: &'static str) -> RoomResult<u64> {
    value.ok_or_else(|| invalid_state(format!("missing {field}")))
}

fn required_f64(value: Option<f64>, field: &'static str) -> RoomResult<f64> {
    value.ok_or_else(|| invalid_state(format!("missing {field}")))
}

fn scale_u64(target_high: u64, baseline: u64, high_baseline: u64) -> RoomResult<u64> {
    if high_baseline == 0 {
        return Err(invalid_state("high baseline max_bitrate is zero"));
    }
    Ok(((target_high as f64 * baseline as f64 / high_baseline as f64).round() as u64).max(1))
}

fn scale_f64(target_high: f64, baseline: f64, high_baseline: f64) -> RoomResult<f64> {
    if high_baseline <= 0.0 {
        return Err(invalid_state("high baseline value must be greater than zero"));
    }
    Ok(target_high * baseline / high_baseline)
}

fn encoding_for_quality(
    encodings: &[RtpEncodingParameters],
    quality: VideoQuality,
) -> RoomResult<&RtpEncodingParameters> {
    let rid = rid_for_quality(quality);
    encodings
        .iter()
        .find(|encoding| encoding.rid == rid)
        .ok_or_else(|| invalid_state(format!("missing baseline simulcast RID '{rid}'")))
}

fn rid_for_quality(quality: VideoQuality) -> &'static str {
    match quality {
        VideoQuality::Low => LOW_RID,
        VideoQuality::Medium => MEDIUM_RID,
        VideoQuality::High => HIGH_RID,
    }
}

fn quality_for_rid(rid: &str) -> Option<VideoQuality> {
    match rid {
        LOW_RID => Some(VideoQuality::Low),
        MEDIUM_RID => Some(VideoQuality::Medium),
        HIGH_RID => Some(VideoQuality::High),
        _ => None,
    }
}

fn format_encoding_parameters(encodings: &[RtpEncodingParameters]) -> String {
    encodings
        .iter()
        .enumerate()
        .map(|(index, encoding)| {
            let rid = if encoding.rid.is_empty() { "-" } else { encoding.rid.as_str() };
            format!(
                "#{index} rid={rid} active={} bitrate={:?} fps={:?} scale={:?} scalability={:?}",
                encoding.active,
                encoding.max_bitrate,
                encoding.max_framerate,
                encoding.scale_resolution_down_by,
                encoding.scalability_mode
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn invalid_state(message: impl Into<String>) -> RoomError {
    RoomError::Rtc(RtcError { error_type: RtcErrorType::InvalidState, message: message.into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoding(
        rid: &str,
        max_bitrate: u64,
        max_framerate: f64,
        scale_resolution_down_by: f64,
    ) -> RtpEncodingParameters {
        RtpEncodingParameters {
            rid: rid.to_string(),
            max_bitrate: Some(max_bitrate),
            max_framerate: Some(max_framerate),
            scale_resolution_down_by: Some(scale_resolution_down_by),
            ..Default::default()
        }
    }

    fn simulcast_baseline() -> Vec<RtpEncodingParameters> {
        vec![
            encoding(HIGH_RID, 1_700_000, 30.0, 1.0),
            encoding(MEDIUM_RID, 450_000, 30.0, 2.0),
            encoding(LOW_RID, 160_000, 30.0, 4.0),
        ]
    }

    fn assert_encoding_matches(encoding: &RtpEncodingParameters, expected: &RtpEncodingParameters) {
        assert_eq!(encoding.rid, expected.rid);
        assert_eq!(encoding.max_bitrate, expected.max_bitrate);
        assert_eq!(encoding.max_framerate, expected.max_framerate);
        assert_eq!(encoding.scale_resolution_down_by, expected.scale_resolution_down_by);
    }

    fn assert_encodings_match(
        encodings: &[RtpEncodingParameters],
        expected: &[RtpEncodingParameters],
    ) {
        assert_eq!(encodings.len(), expected.len());
        for (encoding, expected) in encodings.iter().zip(expected) {
            assert_encoding_matches(encoding, expected);
        }
    }

    #[test]
    fn track_limits_preserve_simulcast_ratios() {
        let baseline = simulcast_baseline();
        let mut encodings = baseline.clone();

        apply_track_encoding_limits(
            &mut encodings,
            &baseline,
            VideoEncodingLimits {
                max_bitrate: Some(850_000),
                max_framerate: Some(15.0),
                scale_resolution_down_by: Some(2.0),
            },
        )
        .expect("track limits should apply");

        assert_eq!(encodings[0].max_bitrate, Some(850_000));
        assert_eq!(encodings[1].max_bitrate, Some(225_000));
        assert_eq!(encodings[2].max_bitrate, Some(80_000));
        assert_eq!(encodings[0].max_framerate, Some(15.0));
        assert_eq!(encodings[1].max_framerate, Some(15.0));
        assert_eq!(encodings[2].max_framerate, Some(15.0));
        assert_eq!(encodings[0].scale_resolution_down_by, Some(2.0));
        assert_eq!(encodings[1].scale_resolution_down_by, Some(4.0));
        assert_eq!(encodings[2].scale_resolution_down_by, Some(8.0));
    }

    #[test]
    fn track_limits_restore_baseline_fields_when_cleared() {
        let baseline = simulcast_baseline();
        let mut encodings = vec![
            encoding(HIGH_RID, 900_000, 10.0, 2.0),
            encoding(MEDIUM_RID, 240_000, 10.0, 4.0),
            encoding(LOW_RID, 85_000, 10.0, 8.0),
        ];

        apply_track_encoding_limits(
            &mut encodings,
            &baseline,
            VideoEncodingLimits {
                max_bitrate: None,
                max_framerate: None,
                scale_resolution_down_by: None,
            },
        )
        .expect("clearing limits should apply");

        assert_encodings_match(&encodings, &baseline);
    }

    #[test]
    fn track_limits_apply_to_single_encoding() {
        let baseline = vec![encoding("", 1_700_000, 30.0, 1.0)];
        let mut encodings = baseline.clone();

        apply_track_encoding_limits(
            &mut encodings,
            &baseline,
            VideoEncodingLimits {
                max_bitrate: Some(900_000),
                max_framerate: None,
                scale_resolution_down_by: Some(2.0),
            },
        )
        .expect("track limits should apply to one encoding");

        assert_eq!(encodings[0].max_bitrate, Some(900_000));
        assert_eq!(encodings[0].max_framerate, Some(30.0));
        assert_eq!(encodings[0].scale_resolution_down_by, Some(2.0));
    }

    #[test]
    fn track_limits_reject_unsupported_simulcast_rid_without_mutating() {
        let baseline = simulcast_baseline();
        let mut encodings = baseline.clone();
        encodings[1].rid = "unknown".to_string();
        let before = encodings.clone();

        let err = apply_track_encoding_limits(
            &mut encodings,
            &baseline,
            VideoEncodingLimits { max_bitrate: Some(850_000), ..Default::default() },
        )
        .expect_err("unsupported simulcast RIDs should fail");

        assert!(matches!(
            err,
            RoomError::Rtc(RtcError { error_type: RtcErrorType::InvalidState, .. })
        ));
        assert_encodings_match(&encodings, &before);
    }
}
