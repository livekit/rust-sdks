// Copyright 2026 LiveKit, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{open_stream, Result, STREAM_TIMEOUT};
use crate::{
    options::{TrackPublishOptions, VideoCodec},
    prelude::LocalTrack,
    rtc_engine::lk_runtime::LkRuntime,
};
use futures_util::StreamExt;
use libwebrtc::{
    audio_source::{native::NativeAudioSource, AudioSourceOptions},
    audio_stream::native::NativeAudioStream,
    media_stream_track::MediaStreamTrack,
    native::{
        audio_codec::{OpusDecoder, OpusEncoder, OpusEncoderConfig},
        video_codec::{
            VideoDecodeError, VideoDecoder, VideoEncodeError, VideoEncoder, VideoEncoderConfig,
        },
    },
    peer_connection_factory::native::PeerConnectionFactoryExt,
    video_frame::{EncodedFrameType, EncodedVideoCodec, EncodedVideoFrame},
    video_source::{native::NativeVideoSource, VideoResolution},
    video_stream::native::NativeVideoStream,
};
use moqtap_client::draft23::connection::FramedSendStream;
use moqtap_codec::{
    dispatch::AnySubgroupHeader,
    draft23::data_stream::{SubgroupHeader, SubgroupObject},
    object_properties::{parse_object_properties, PropertyValue},
    varint::{Moqt18, VarInt},
    version::DraftVersion,
};
use serde::Deserialize;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

#[derive(Deserialize)]
pub(super) struct Catalog {
    pub version: String,
    pub tracks: Vec<Rendition>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Rendition {
    pub name: String,
    pub codec: String,
    pub packaging: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub channel_config: String,
    pub livekit: Option<TrackIds>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TrackIds {
    pub sid: String,
    pub publisher_sid: String,
}

pub(super) struct Sample {
    pub group: u64,
    pub object: SubgroupObject,
}

struct MediaWriter {
    raw: quinn::Connection,
    alias: VarInt,
    group: u64,
    object: u64,
    stream: Option<FramedSendStream>,
}

impl MediaWriter {
    async fn send(
        &mut self,
        payload: Vec<u8>,
        timestamp: u64,
        key: bool,
        audio_level: Option<u8>,
    ) -> Result<()> {
        if payload.is_empty() || payload.len() > super::MAX_MEDIA {
            return Err("invalid encoded media size".into());
        }
        let video = audio_level.is_none();
        if key {
            if let Some(mut previous) = self.stream.take() {
                previous.finish().await?
            }
            self.group += 1;
            self.object = 0;
            let header = SubgroupHeader {
                header_type: 0x19,
                track_alias: self.alias,
                group_id: VarInt::from_u64(self.group)?,
                subgroup_id: VarInt::from_u64(0)?,
                publisher_priority: Some(if video { 80 } else { 32 }),
            };
            let mut stream = open_stream(&self.raw).await?;
            stream.write_subgroup_header(&AnySubgroupHeader::Draft23(header)).await?;
            self.stream = Some(stream);
        }
        let Some(stream) = &mut self.stream else { return Ok(()) };
        // Property IDs are delta encoded. Both paths end at the LOC timestamp (0x10).
        let mut properties = if let Some(level) = audio_level {
            let mut properties = vec![0x0c];
            VarInt::from_u64_moqt(u64::from(level)).encode_moqt::<Moqt18>(&mut properties);
            properties.push(0x10 - 0x0c);
            properties
        } else {
            vec![9, 1, if key { 0xe0 } else { 0xc0 }, 7]
        };
        VarInt::from_u64_moqt(timestamp).encode_moqt::<Moqt18>(&mut properties);
        stream
            .write_subgroup_object(&SubgroupObject {
                object_id: VarInt::from_u64(self.object)?,
                extension_headers: properties,
                payload_length: VarInt::from_u64(payload.len() as u64)?,
                object_status: None,
                payload,
            })
            .await?;
        self.object += 1;
        if !video {
            if let Some(mut stream) = self.stream.take() {
                stream.finish().await?;
            }
        }
        Ok(())
    }
}

pub(super) enum Encoder {
    Audio(NativeAudioStream, OpusEncoder),
    Video(NativeVideoStream, VideoEncoder),
}

impl Encoder {
    pub(super) fn new(track: &LocalTrack, options: &TrackPublishOptions) -> Result<Self> {
        match track {
            LocalTrack::Audio(t) => {
                let channels = t.rtc_source().num_channels().clamp(1, 2);
                let config = OpusEncoderConfig {
                    num_channels: channels,
                    stereo: channels == 2,
                    dtx: options.dtx,
                    bitrate_bps: options
                        .audio_encoding
                        .as_ref()
                        .map_or(0, |e| e.max_bitrate.min(u32::MAX as u64) as u32),
                    ..Default::default()
                };
                Ok(Self::Audio(
                    NativeAudioStream::new(t.rtc_track(), 48000, channels as i32),
                    OpusEncoder::new(&config)?,
                ))
            }
            LocalTrack::Video(t) => {
                let size = t.rtc_source().video_resolution();
                let codec = match options.video_codec {
                    VideoCodec::VP8 => EncodedVideoCodec::VP8,
                    VideoCodec::VP9 => EncodedVideoCodec::VP9,
                    VideoCodec::H264 => EncodedVideoCodec::H264,
                    VideoCodec::H265 => EncodedVideoCodec::H265,
                    VideoCodec::AV1 => EncodedVideoCodec::AV1,
                };
                let mut config = VideoEncoderConfig::new(codec, size.width, size.height, 1_500_000);
                config.backend = options.video_encoder;
                if let Some(encoding) = &options.video_encoding {
                    config.max_bitrate_bps = u32::try_from(encoding.max_bitrate)?;
                    config.max_framerate = encoding.max_framerate;
                }
                Ok(Self::Video(NativeVideoStream::new(t.rtc_track()), VideoEncoder::new(&config)?))
            }
        }
    }

    pub(super) async fn run(mut self, raw: quinn::Connection, alias: VarInt) -> Result<()> {
        let mut out = MediaWriter { raw, alias, group: 0, object: 0, stream: None };
        let epoch = SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros() as u64;
        let start = Instant::now();
        match &mut self {
            Self::Audio(source, encoder) => {
                while let Some(frame) = source.next().await {
                    if let Some(packet) = encoder.encode(&frame)? {
                        let time = epoch + start.elapsed().as_micros() as u64;
                        tokio::time::timeout(
                            STREAM_TIMEOUT,
                            out.send(packet.data, time, true, Some(packet.audio_level)),
                        )
                        .await??;
                    }
                }
            }
            Self::Video(source, encoder) => {
                let mut last_keyframe_request = Instant::now();
                loop {
                    tokio::select! {
                        frame = source.next() => {
                            let Some(frame) = frame else { break };
                            if last_keyframe_request.elapsed() >= Duration::from_secs(2) {
                                encoder.request_keyframe(None);
                                last_keyframe_request = Instant::now();
                            }
                            match encoder.encode(&frame) {
                                Ok(()) | Err(VideoEncodeError::Busy) => {}
                                Err(err) => return Err(err.into()),
                            }
                        }
                        encoded = encoder.recv() => {
                            let Some(frame) = encoded else { break };
                            let key = frame.is_keyframe();
                            let timestamp = epoch + start.elapsed().as_micros() as u64;
                            tokio::time::timeout(
                                STREAM_TIMEOUT,
                                out.send(frame.data, timestamp, key, None),
                            ).await??;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

pub(super) enum Decoder {
    Audio(NativeAudioSource, OpusDecoder),
    Video(NativeVideoSource, VideoDecoder, VideoResolution, EncodedVideoCodec),
}

impl Decoder {
    pub(super) fn new(sid: &str, track: &Rendition) -> Result<(Self, MediaStreamTrack)> {
        let runtime = LkRuntime::instance();
        match track.codec.as_str() {
            "opus" => {
                let channels = if track.channel_config == "2" { 2 } else { 1 };
                let source =
                    NativeAudioSource::new(AudioSourceOptions::default(), 48000, channels, 100);
                let rtc = runtime.pc_factory().create_audio_track(sid, source.clone());
                Ok((
                    Self::Audio(source, OpusDecoder::new(48000, channels)?),
                    MediaStreamTrack::Audio(rtc),
                ))
            }
            name => {
                let codec = video_codec(name).ok_or("unsupported LOC codec")?;
                if track.width > 8192
                    || track.height > 8192
                    || u64::from(track.width) * u64::from(track.height) > 8192 * 4320
                {
                    return Err("video dimensions exceed the decode limit".into());
                }
                let size = VideoResolution { width: track.width, height: track.height };
                let source = NativeVideoSource::new_encoded(size.clone());
                let rtc = runtime.pc_factory().create_video_track(sid, source.clone());
                Ok((
                    Self::Video(source, VideoDecoder::new(codec)?, size, codec),
                    MediaStreamTrack::Video(rtc),
                ))
            }
        }
    }

    pub(super) async fn run(mut self, mut rx: mpsc::Receiver<Sample>) -> Result<()> {
        let mut latest = None;
        let mut need_keyframe = true;
        while let Some(s) = rx.recv().await {
            let pos = (s.group, s.object.object_id.into_inner());
            if latest.is_some_and(|last| pos <= last) {
                continue;
            }
            match &mut self {
                Self::Audio(source, decoder) => {
                    let pcm = decoder.decode(&s.object.payload)?;
                    source.capture_frame(&pcm).await?;
                }
                Self::Video(source, decoder, size, codec) => {
                    let key = pos.1 == 0;
                    if !key && (need_keyframe || latest != Some((pos.0, pos.1 - 1))) {
                        continue;
                    }
                    let props = parse_object_properties(
                        DraftVersion::Draft23,
                        &s.object.extension_headers,
                    )?;
                    let ts = props
                        .into_iter()
                        .find_map(|p| match (p.property_type, p.value) {
                            (16, PropertyValue::Varint(n)) => i64::try_from(n).ok(),
                            _ => None,
                        })
                        .ok_or("missing LOC timestamp")?;
                    let frame = EncodedVideoFrame {
                        codec: *codec,
                        payload: &s.object.payload,
                        timestamp_us: ts,
                        frame_type: if key {
                            EncodedFrameType::Key
                        } else {
                            EncodedFrameType::Delta
                        },
                        resolution: size.clone(),
                        frame_metadata: None,
                    };
                    match decoder.decode(&frame) {
                        Ok(()) => need_keyframe = false,
                        Err(VideoDecodeError::KeyframeRequired) => {
                            need_keyframe = true;
                            latest = Some(pos);
                            continue;
                        }
                        Err(err) => return Err(err.into()),
                    }
                    // A dropped input can produce no output. Keep the receive worker cancellable.
                    if let Ok(Some(frame)) =
                        tokio::time::timeout(Duration::from_millis(100), decoder.recv()).await
                    {
                        source.capture_frame(&frame);
                    }
                }
            }
            latest = Some(pos);
        }
        Ok(())
    }
}

pub(super) fn video_codec(name: &str) -> Option<EncodedVideoCodec> {
    match name.split('.').next()? {
        "avc1" | "avc3" => Some(EncodedVideoCodec::H264),
        "hev1" | "hvc1" => Some(EncodedVideoCodec::H265),
        "vp8" => Some(EncodedVideoCodec::VP8),
        "vp09" => Some(EncodedVideoCodec::VP9),
        "av01" => Some(EncodedVideoCodec::AV1),
        _ => None,
    }
}
