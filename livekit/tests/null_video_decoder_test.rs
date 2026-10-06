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

#[cfg(feature = "__lk-e2e-test")]
use {anyhow::Result, common::decode_stats::run_codec_matrix};

mod common;

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn null_video_decoder_keeps_inbound_stats() -> Result<()> {
    livekit::webrtc::enable_null_video_decoder()?;
    run_codec_matrix(|decoder| decoder == "NullVideoDecoder").await
}
