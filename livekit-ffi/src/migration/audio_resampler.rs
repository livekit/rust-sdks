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

//! Everything holding [`crate::server::audio_resampler::AudioResampler`] onto the protobuf
//! path: the handle bridge and the two request handlers.
//!
//! This whole file goes when the protobuf path does — sooner, if `RemixAndResample` is
//! retired first, which it is marked to be.

use std::slice;

use crate::server::audio_resampler::{AudioResampler, AudioResamplerError};
use crate::server::audio_stream::AudioFrameBuffer;
use crate::{proto, server, FfiError, FfiResult};

crate::migrate_from_ffi!(AudioResampler);

impl From<AudioResamplerError> for FfiError {
    fn from(err: AudioResamplerError) -> Self {
        FfiError::InvalidRequest(err.to_string().into())
    }
}

impl AudioResampler {
    pub fn new_ffi() -> proto::NewAudioResamplerResponse {
        let handle_id = Self::new().ffi_handle_id();
        proto::NewAudioResamplerResponse {
            resampler: proto::OwnedAudioResampler {
                handle: proto::FfiOwnedHandle { id: handle_id },
                info: proto::AudioResamplerInfo {},
            },
        }
    }

    pub fn remix_and_resample_ffi(
        server: &'static server::FfiServer,
        remix: proto::RemixAndResampleRequest,
    ) -> FfiResult<proto::RemixAndResampleResponse> {
        let resampler =
            server.retrieve_handle::<std::sync::Arc<Self>>(remix.resampler_handle)?.clone();

        let buffer = remix.buffer;
        // The caller owns these samples for the length of the request, which is longer
        // than the copy below.
        let data = unsafe {
            slice::from_raw_parts(
                buffer.data_ptr as *const i16,
                (buffer.num_channels * buffer.samples_per_channel) as usize,
            )
        }
        .to_vec();

        let resampled = resampler.remix_and_resample(
            AudioFrameBuffer {
                sample_rate: buffer.sample_rate,
                num_channels: buffer.num_channels,
                samples_per_channel: buffer.samples_per_channel,
                data,
            },
            remix.num_channels,
            remix.sample_rate,
        )?;

        let (handle_id, info) = resampled.into_ffi(server);
        Ok(proto::RemixAndResampleResponse {
            buffer: proto::OwnedAudioFrameBuffer {
                handle: proto::FfiOwnedHandle { id: handle_id },
                info,
            },
        })
    }
}
