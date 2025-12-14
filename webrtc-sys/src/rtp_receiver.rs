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

use tokio::sync::mpsc;

use crate::{
    RtcError, RtcErrorType, media_stream_track::MediaStreamTrack, rtp_parameters::RtpParameters, stats::RtcStats, sys
};

#[derive(Clone)]
pub struct RtpReceiver {
    pub ffi: sys::RefCounted<crate::sys::lkRtpReceiver>,
}

impl RtpReceiver {
    pub fn from_native(ffi: sys::RefCounted<sys::lkRtpReceiver>) -> Self {
        Self { ffi }
    }

    pub fn track(&self) -> Option<MediaStreamTrack> {
        unsafe {
            let track_ptr = sys::lkRtpReceiverGetTrack(self.ffi.as_ptr());
            if track_ptr.is_null() {
                None
            } else {
                Some(crate::media_stream_track::new_media_stream_track(unsafe {
                    sys::RefCounted::from_raw(track_ptr)
                }))
            }
        }
    }

    pub async fn get_stats(&self) -> Result<Vec<RtcStats>, RtcError> {
        let (tx, mut rx) = mpsc::channel::<Result<Vec<RtcStats>, RtcError>>(1);
        let tx_box = Box::new(tx.clone());
        let userdata = Box::into_raw(tx_box) as *mut std::ffi::c_void;

        unsafe extern "C" fn on_complete(
            stats_json: *const ::std::os::raw::c_char,
            userdata: *mut ::std::os::raw::c_void,
        ) {
            let tx: Box<mpsc::Sender<Result<Vec<RtcStats>, RtcError>>> = Box::from_raw(userdata as *mut _);
            let stats = unsafe { std::ffi::CStr::from_ptr(stats_json) };

            if stats.is_empty() {
                let _ = tx.send(Ok(vec![]));
                return;
            }

            let vec = serde_json::from_str(stats.to_str().unwrap()).unwrap();
            let _ = tx.blocking_send(Ok(vec));
        }

        unsafe {
            sys::lkRtpReceiverGetStats(
                self.ffi.as_ptr(),
                Some(on_complete),
                userdata,
            );
        }

        rx.recv().await.ok_or_else(|| RtcError {
            error_type: RtcErrorType::Internal,
            message: "get_stats cancelled".to_owned(),
        })?
    }

    pub fn parameters(&self) -> RtpParameters {
        todo!()
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
