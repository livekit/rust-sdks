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
    fmt::{Debug, Formatter},
    sync::Arc,
};

use libwebrtc::prelude::*;
use livekit_protocol as proto;
use parking_lot::Mutex;
use tokio::sync::Mutex as AsyncMutex;

use super::EngineResult;

pub type OnOfferCreated = Box<dyn FnMut(SessionDescription) + Send + Sync>;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
const DEFAULT_VP9_START_BITRATE_KBPS: u32 = 2500;

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
const DEFAULT_VP9_START_BITRATE_KBPS: u32 = 0; // 0 means “don’t apply”

struct TransportInner {
    pending_candidates: Vec<IceCandidate>,
    renegotiate: bool,
    restarting_ice: bool,
}

pub struct PeerTransport {
    signal_target: proto::SignalTarget,
    peer_connection: PeerConnection,
    on_offer_handler: Mutex<Option<OnOfferCreated>>,
    inner: Arc<AsyncMutex<TransportInner>>,
}

impl Debug for PeerTransport {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("PeerTransport").field("target", &self.signal_target).finish()
    }
}

impl PeerTransport {
    pub fn new(peer_connection: PeerConnection, signal_target: proto::SignalTarget) -> Self {
        Self {
            signal_target,
            peer_connection,
            on_offer_handler: Mutex::new(None),
            inner: Arc::new(AsyncMutex::new(TransportInner {
                pending_candidates: Vec::default(),
                renegotiate: false,
                restarting_ice: false,
            })),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.peer_connection.connection_state() == PeerConnectionState::Connected
    }

    pub fn peer_connection(&self) -> PeerConnection {
        self.peer_connection.clone()
    }

    pub fn signal_target(&self) -> proto::SignalTarget {
        self.signal_target
    }

    pub fn on_offer(&self, handler: Option<OnOfferCreated>) {
        *self.on_offer_handler.lock() = handler;
    }

    pub fn close(&self) {
        self.peer_connection.close();
    }

    pub async fn add_ice_candidate(&self, ice_candidate: IceCandidate) -> EngineResult<()> {
        let mut inner = self.inner.lock().await;

        if self.peer_connection.current_remote_description().is_some() && !inner.restarting_ice {
            drop(inner);
            self.peer_connection.add_ice_candidate(ice_candidate).await?;

            return Ok(());
        }

        inner.pending_candidates.push(ice_candidate);
        Ok(())
    }

    pub async fn set_remote_description(
        &self,
        remote_description: SessionDescription,
    ) -> EngineResult<()> {
        let mut inner = self.inner.lock().await;

        self.peer_connection.set_remote_description(remote_description).await?;

        for ic in inner.pending_candidates.drain(..) {
            self.peer_connection.add_ice_candidate(ic).await?;
        }

        inner.restarting_ice = false;

        if inner.renegotiate {
            inner.renegotiate = false;
            self.create_and_send_offer(OfferOptions::default()).await?;
        }

        Ok(())
    }

    pub async fn create_anwser(
        &self,
        offer: SessionDescription,
        options: AnswerOptions,
    ) -> EngineResult<SessionDescription> {
        self.set_remote_description(offer).await?;
        let answer = self.peer_connection().create_answer(options).await?;
        self.peer_connection().set_local_description(answer.clone()).await?;

        Ok(answer)
    }

    fn munge_x_google_start_bitrate(sdp: &str, start_bitrate_kbps: u32) -> String {
        // 1) Find payload types (PTs) for VP9 / AV1 from a=rtpmap:<pt> VP9/90000 etc
        let mut target_pts: Vec<String> = Vec::new();
        for line in sdp.lines() {
            let l = line.trim();
            if let Some(rest) = l.strip_prefix("a=rtpmap:") {
                // rest looks like: "<pt> VP9/90000"
                let mut it = rest.split_whitespace();
                let pt = it.next().unwrap_or("");
                let codec = it.next().unwrap_or("");
                if codec.starts_with("VP9/90000") || codec.starts_with("AV1/90000") {
                    if !pt.is_empty() {
                        target_pts.push(pt.to_string());
                    }
                }
            }
        }
        if target_pts.is_empty() {
            return sdp.to_string();
        }

        // 2) For each PT, ensure a=fmtp:<pt> has x-google-start-bitrate=...
        // We do a line-by-line rewrite. If there is no fmtp line for that PT, we leave it alone
        // (safer; avoids adding new fmtp that might not be accepted).
        let mut out: Vec<String> = Vec::with_capacity(sdp.lines().count());
        for line in sdp.lines() {
            let mut rewritten = line.to_string();

            for pt in &target_pts {
                let prefix = format!("a=fmtp:{pt} ");
                if rewritten.starts_with(&prefix) {
                    // Only append if not already present
                    if !rewritten.contains("x-google-start-bitrate=") {
                        rewritten
                            .push_str(&format!(";x-google-start-bitrate={start_bitrate_kbps}"));
                    }
                    break;
                }
            }

            out.push(rewritten);
        }

        out.join("\r\n")
    }

    pub async fn create_and_send_offer(&self, options: OfferOptions) -> EngineResult<()> {
        let mut inner = self.inner.lock().await;

        if options.ice_restart {
            inner.restarting_ice = true;
        }

        if self.peer_connection.signaling_state() == SignalingState::HaveLocalOffer {
            let remote_sdp = self.peer_connection.current_remote_description();
            if options.ice_restart && remote_sdp.is_some() {
                let remote_sdp = remote_sdp.unwrap();

                // Cancel the old renegotiation (Basically say the server rejected the previous
                // offer) So we can resend a new offer just after this
                self.peer_connection.set_remote_description(remote_sdp).await?;
            } else {
                inner.renegotiate = true;
                return Ok(());
            }
        } else if self.peer_connection.signaling_state() == SignalingState::Closed {
            log::warn!("peer connection is closed, cannot create offer");
            return Ok(());
        }

        let mut offer = self.peer_connection.create_offer(options).await?;
        let start_bitrate_kbps = DEFAULT_VP9_START_BITRATE_KBPS;
        let sdp = offer.to_string();
        // TODO, we should extend the codec support to AV1 ?
        if start_bitrate_kbps > 0 && sdp.contains(" VP9/90000") {
            let munged = Self::munge_x_google_start_bitrate(&sdp, start_bitrate_kbps);
            if munged != sdp {
                match SessionDescription::parse(&munged, offer.sdp_type()) {
                    Ok(parsed) => {
                        offer = parsed;
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to parse munged SDP, falling back to original offer: {e}"
                        );
                    }
                }
            }
        }
        self.peer_connection.set_local_description(offer.clone()).await?;

        if let Some(handler) = self.on_offer_handler.lock().as_mut() {
            handler(offer);
        }

        Ok(())
    }
}
