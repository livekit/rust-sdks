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

use std::fmt::Debug;
use std::time::Duration;

use crate::{
    imp::rtp_receiver as imp_rr, media_stream_track::MediaStreamTrack,
    rtp_parameters::RtpParameters, stats::RtcStats, RtcError,
};

#[derive(Clone)]
pub struct RtpReceiver {
    pub(crate) handle: imp_rr::RtpReceiver,
}

impl RtpReceiver {
    pub fn track(&self) -> Option<MediaStreamTrack> {
        self.handle.track()
    }

    pub async fn get_stats(&self) -> Result<Vec<RtcStats>, RtcError> {
        self.handle.get_stats().await
    }

    pub fn parameters(&self) -> RtpParameters {
        self.handle.parameters()
    }

    /// Sets an application-requested lower bound for the receiver jitter buffer.
    ///
    /// On the currently bound native WebRTC API this is the only playout-latency
    /// control available on `RtpReceiver`. Internal WebRTC code has richer
    /// video playout-delay concepts, but they are not surfaced through this
    /// SDK's receiver bindings yet.
    ///
    /// Passing `None` clears the override and restores default receiver
    /// behavior. Passing `Some(Duration::ZERO)` requests the lowest allowed
    /// playout floor without forcing an additional delay.
    pub fn set_jitter_buffer_minimum_delay(&self, delay: Option<Duration>) {
        self.handle.set_jitter_buffer_minimum_delay(delay.map(|delay| delay.as_secs_f64()));
    }
}

impl Debug for RtpReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtpReceiver")
            .field("track", &self.track())
            .field("cname", &self.parameters().rtcp.cname)
            .finish()
    }
}
