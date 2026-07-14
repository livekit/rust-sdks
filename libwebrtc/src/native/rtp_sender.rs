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

use cxx::SharedPtr;
use tokio::sync::oneshot;
use webrtc_sys::{rtc_error as sys_err, rtp_sender as sys_rs, webrtc as sys_webrtc};

use super::media_stream_track::new_media_stream_track;
use crate::{
    media_stream_track::MediaStreamTrack, rtp_parameters::RtpParameters,
    rtp_sender::VideoEncoderBackend, stats::RtcStats, RtcError, RtcErrorType,
};

#[derive(Clone)]
pub struct RtpSender {
    pub(crate) sys_handle: SharedPtr<sys_rs::ffi::RtpSender>,
}

impl RtpSender {
    pub fn track(&self) -> Option<MediaStreamTrack> {
        let track_handle = self.sys_handle.track();
        if track_handle.is_null() {
            return None;
        }

        Some(new_media_stream_track(track_handle))
    }

    pub async fn get_stats(&self) -> Result<Vec<RtcStats>, RtcError> {
        let (tx, rx) = oneshot::channel::<Result<Vec<RtcStats>, RtcError>>();
        let ctx = Box::new(sys_rs::SenderContext(Box::new(tx)));

        self.sys_handle.get_stats(ctx, |ctx, stats| {
            let tx = ctx.0.downcast::<oneshot::Sender<Result<Vec<RtcStats>, RtcError>>>().unwrap();

            if stats.is_empty() {
                let _ = tx.send(Ok(vec![]));
                return;
            }

            // Unwrap because it should not happens
            let vec = serde_json::from_str(&stats).unwrap();
            let _ = tx.send(Ok(vec));
        });

        rx.await.map_err(|_| RtcError {
            error_type: RtcErrorType::Internal,
            message: "get_stats cancelled".to_owned(),
        })?
    }

    pub fn set_track(&self, track: Option<MediaStreamTrack>) -> Result<(), RtcError> {
        if !self.sys_handle.set_track(track.map_or(SharedPtr::null(), |t| t.sys_handle())) {
            return Err(RtcError {
                error_type: RtcErrorType::InvalidState,
                message: "Failed to set track".to_string(),
            });
        }

        Ok(())
    }

    pub fn parameters(&self) -> RtpParameters {
        self.sys_handle.get_parameters().into()
    }

    pub fn set_parameters(&self, parameters: RtpParameters) -> Result<(), RtcError> {
        self.sys_handle
            .set_parameters(parameters.into())
            .map_err(|e| unsafe { sys_err::ffi::RtcError::from(e.what()).into() })
    }

    pub fn set_video_encoder_backend(&self, backend: VideoEncoderBackend) {
        self.sys_handle.set_video_encoder_backend(backend.into());
    }
}

impl From<VideoEncoderBackend> for sys_webrtc::ffi::VideoEncoderBackend {
    fn from(value: VideoEncoderBackend) -> Self {
        match value {
            VideoEncoderBackend::Auto => Self::Auto,
            VideoEncoderBackend::Software => Self::Software,
            VideoEncoderBackend::Hardware => Self::Hardware,
            VideoEncoderBackend::Nvenc => Self::Nvenc,
            VideoEncoderBackend::Vaapi => Self::Vaapi,
            VideoEncoderBackend::VideoToolbox => Self::VideoToolbox,
            VideoEncoderBackend::PreEncoded => Self::PreEncoded,
        }
    }
}

impl From<sys_webrtc::ffi::VideoEncoderBackend> for VideoEncoderBackend {
    fn from(value: sys_webrtc::ffi::VideoEncoderBackend) -> Self {
        match value {
            sys_webrtc::ffi::VideoEncoderBackend::Auto => Self::Auto,
            sys_webrtc::ffi::VideoEncoderBackend::Software => Self::Software,
            sys_webrtc::ffi::VideoEncoderBackend::Hardware => Self::Hardware,
            sys_webrtc::ffi::VideoEncoderBackend::Nvenc => Self::Nvenc,
            sys_webrtc::ffi::VideoEncoderBackend::Vaapi => Self::Vaapi,
            sys_webrtc::ffi::VideoEncoderBackend::VideoToolbox => Self::VideoToolbox,
            sys_webrtc::ffi::VideoEncoderBackend::PreEncoded => Self::PreEncoded,
            _ => panic!("unknown VideoEncoderBackend"),
        }
    }
}

pub fn video_encoder_backend_list() -> Vec<VideoEncoderBackend> {
    sys_webrtc::ffi::video_encoder_backend_list().into_iter().map(Into::into).collect()
}

#[cfg(test)]
mod tests {
    use super::{sys_webrtc, VideoEncoderBackend};

    #[test]
    fn video_encoder_backend_maps_to_native_enum() {
        let cases = [
            (VideoEncoderBackend::Auto, sys_webrtc::ffi::VideoEncoderBackend::Auto),
            (VideoEncoderBackend::Software, sys_webrtc::ffi::VideoEncoderBackend::Software),
            (VideoEncoderBackend::Hardware, sys_webrtc::ffi::VideoEncoderBackend::Hardware),
            (VideoEncoderBackend::Nvenc, sys_webrtc::ffi::VideoEncoderBackend::Nvenc),
            (VideoEncoderBackend::Vaapi, sys_webrtc::ffi::VideoEncoderBackend::Vaapi),
            (VideoEncoderBackend::VideoToolbox, sys_webrtc::ffi::VideoEncoderBackend::VideoToolbox),
            (VideoEncoderBackend::PreEncoded, sys_webrtc::ffi::VideoEncoderBackend::PreEncoded),
        ];

        for (backend, expected) in cases {
            assert_eq!(sys_webrtc::ffi::VideoEncoderBackend::from(backend), expected);
            assert_eq!(VideoEncoderBackend::from(expected), backend);
        }
    }
}
