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
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use libwebrtc::prelude::*;
use livekit_protocol as proto;
use parking_lot::Mutex;
use tokio::sync::Mutex as AsyncMutex;

use super::EngineResult;

pub type OnOfferCreated = Box<dyn FnMut(SessionDescription) + Send + Sync>;

struct TransportInner {
    pending_candidates: Vec<IceCandidate>,
    renegotiate: bool,
    restarting_ice: bool,
    single_pc_mode: bool,
    // Publish-side target bitrate (bps) for offer munging
    max_send_bitrate_bps: Option<u64>,
    pending_initial_offer: Option<SessionDescription>,
}

pub struct PeerTransport {
    signal_target: proto::SignalTarget,
    peer_connection: PeerConnection,
    on_offer_handler: Mutex<Option<OnOfferCreated>>,
    inner: Arc<AsyncMutex<TransportInner>>,

    /// Incremented every time a remote description is applied to this transport, i.e. every
    /// time a negotiation round-trip completes (publisher answers, subscriber offers).
    ///
    /// Together with [`Self::disconnect_generation`] this lets a resume decide whether a
    /// transport reporting `Connected` did so *because it recovered* or merely because
    /// libwebrtc has not yet noticed the far end is gone. See
    /// [`super::rtc_session::SessionInner::wait_pc_reconnected_with_snapshot`].
    negotiation_generation: AtomicU32,

    /// Incremented every time the PeerConnection leaves `Connected`.
    ///
    /// Distinguishes "still `Connected` because nothing ever broke" from "was `Connected`,
    /// broke, and came back" — a level read of the connection state cannot tell them apart.
    disconnect_generation: AtomicU32,
}

impl Debug for PeerTransport {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("PeerTransport").field("target", &self.signal_target).finish()
    }
}

impl PeerTransport {
    pub fn new(
        peer_connection: PeerConnection,
        signal_target: proto::SignalTarget,
        single_pc_mode: bool,
    ) -> Self {
        Self {
            signal_target,
            peer_connection,
            on_offer_handler: Mutex::new(None),
            inner: Arc::new(AsyncMutex::new(TransportInner {
                pending_candidates: Vec::default(),
                renegotiate: false,
                restarting_ice: false,
                single_pc_mode,
                max_send_bitrate_bps: None,
                pending_initial_offer: None,
            })),
            negotiation_generation: AtomicU32::new(0),
            disconnect_generation: AtomicU32::new(0),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.peer_connection.connection_state() == PeerConnectionState::Connected
    }

    /// See [`Self::negotiation_generation`].
    pub fn negotiation_generation(&self) -> u32 {
        self.negotiation_generation.load(Ordering::Acquire)
    }

    /// See [`Self::disconnect_generation`].
    pub fn disconnect_generation(&self) -> u32 {
        self.disconnect_generation.load(Ordering::Acquire)
    }

    /// Mark this transport as awaiting a fresh ICE generation from the remote side.
    ///
    /// The subscriber never issues its own offer — the SFU does — so it has no
    /// `create_and_send_offer(ice_restart)` call to set this flag. Without it, remote
    /// candidates for the *new* generation that arrive before the SFU's offer are applied
    /// against the old, dead remote description instead of being queued and replayed once
    /// the new offer lands. Mirrors `subscriber.restartingIce = true` in client-sdk-js's
    /// `PCTransportManager.triggerIceRestart`.
    ///
    /// This is speculative in a way the publisher's equivalent is not: the publisher sets the
    /// flag alongside an offer it will certainly receive an answer to, whereas here we are
    /// only *expecting* the SFU to re-offer. It therefore MUST be paired with
    /// [`Self::finish_restarting_ice`] to bound the window, or a resume where the SFU never
    /// re-offers would leave the flag set for the lifetime of the transport.
    pub async fn mark_restarting_ice(&self) {
        self.inner.lock().await.restarting_ice = true;
    }

    /// End the window opened by [`Self::mark_restarting_ice`], applying anything that queued
    /// while it was open.
    ///
    /// [`Self::set_remote_description`] already does this when the expected description
    /// arrives, so in that case this is a no-op. It exists for the case where the expected
    /// description never comes: the SFU only re-offers the subscriber when the resume
    /// actually moved us to a new node, so after an ordinary signal-only resume the flag
    /// would otherwise stay set forever and every subsequent remote candidate would be
    /// buffered instead of applied — leaving the transport unable to adopt any new network
    /// path the server proposes.
    ///
    /// NOTE: a resume cancelled mid-flight by an engine close never reaches this, leaving the
    /// window open. Sound today because an engine close is terminal, so the transport is never
    /// used again; see the note in `EngineInner::try_resume_connection` for what would have to
    /// change for that to matter.
    pub async fn finish_restarting_ice(&self) -> EngineResult<()> {
        let mut inner = self.inner.lock().await;
        if !inner.restarting_ice {
            return Ok(());
        }
        inner.restarting_ice = false;

        // No fresh description arrived, so anything queued belongs to the generation that is
        // still current and can be applied exactly as it would have been before the resume.
        // With no description to apply them against, leave them queued as
        // `add_ice_candidate` itself would.
        if self.peer_connection.current_remote_description().is_none() {
            return Ok(());
        }

        for ic in inner.pending_candidates.drain(..) {
            self.peer_connection.add_ice_candidate(ic).await?;
        }

        Ok(())
    }

    /// Record a PeerConnection state transition observed by the session.
    ///
    /// Called for every `RtcEvent::ConnectionChange` on this transport's target so that
    /// leaving `Connected` is durably recorded, rather than having to be caught by whoever
    /// happens to be polling at that instant.
    pub fn note_connection_state(&self, state: PeerConnectionState) {
        if state != PeerConnectionState::Connected {
            self.disconnect_generation.fetch_add(1, Ordering::AcqRel);
        }
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
        if let Ok(mut inner) = self.inner.try_lock() {
            inner.pending_initial_offer = None;
        }
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

        if let Some(pending_offer) = inner.pending_initial_offer.take() {
            self.peer_connection.set_local_description(pending_offer).await?;
        }

        self.peer_connection.set_remote_description(remote_description).await?;

        // A negotiation round-trip completed on this transport. Recorded *after* the
        // description is applied, so the generation only advances on success.
        //
        // Note the rollback in `create_and_send_offer` deliberately calls
        // `peer_connection.set_remote_description` directly rather than going through this
        // method, so re-applying the existing remote description does not count as a fresh
        // negotiation.
        self.negotiation_generation.fetch_add(1, Ordering::AcqRel);

        for ic in inner.pending_candidates.drain(..) {
            self.peer_connection.add_ice_candidate(ic).await?;
        }

        inner.restarting_ice = false;

        if inner.renegotiate {
            inner.renegotiate = false;
            // Release the lock before re-entering `create_and_send_offer`, which re-acquires
            // `self.inner`. `tokio::sync::Mutex` is not reentrant, so holding the guard across
            // this call would deadlock the task.
            drop(inner);
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

    /// Create an initial offer without setting it as local description.
    /// The offer is stored as pending and will be applied when the server's answer arrives.
    ///
    /// In single PC mode, this initial offer is sent with the JoinRequest before any track
    /// is published. We apply both `inactive→recvonly` munging and `x-google-start-bitrate`
    /// munging when a target bitrate is known.
    pub async fn create_initial_offer(&self) -> EngineResult<Option<SessionDescription>> {
        let inner = self.inner.lock().await;
        if !inner.single_pc_mode {
            return Ok(None);
        }
        drop(inner);

        let mut offer = self.peer_connection.create_offer(OfferOptions::default()).await?;
        let mut sdp = offer.to_string();

        // Apply inactive→recvonly munging for single PC mode
        let recvonly_munged = Self::munge_inactive_to_recvonly_for_media(&sdp);
        if recvonly_munged != sdp {
            if let Ok(parsed) = SessionDescription::parse(&recvonly_munged, offer.sdp_type()) {
                offer = parsed;
                sdp = recvonly_munged;
            }
        }

        // Apply x-google-start-bitrate munging for video codecs if we have a target bitrate.
        // In initial offers (before track is published), max_send_bitrate_bps is None,
        // so no munging is applied and WebRTC uses its default conservative start bitrate.
        let has_video = sdp.contains(" VP8/90000")
            || sdp.contains(" VP9/90000")
            || sdp.contains(" AV1/90000")
            || sdp.contains(" H264/90000")
            || sdp.contains(" H265/90000");
        if has_video {
            let start_kbps = {
                let inner = self.inner.lock().await;
                Self::compute_start_bitrate_kbps(inner.max_send_bitrate_bps)
            };
            if let Some(start_kbps) = start_kbps {
                log::info!("Initial offer: applying x-google-start-bitrate={} kbps", start_kbps);

                let munged = Self::munge_x_google_start_bitrate(&sdp, start_kbps);
                if munged != sdp {
                    if let Ok(parsed) = SessionDescription::parse(&munged, offer.sdp_type()) {
                        offer = parsed;
                    }
                }
            }
        }

        let mut inner = self.inner.lock().await;
        inner.pending_initial_offer = Some(offer.clone());
        Ok(Some(offer))
    }

    pub async fn clear_pending_initial_offer(&self) {
        let mut inner = self.inner.lock().await;
        inner.pending_initial_offer = None;
    }

    pub async fn set_max_send_bitrate_bps(&self, bps: Option<u64>) {
        let mut inner = self.inner.lock().await;
        inner.max_send_bitrate_bps = bps;
    }

    /// Maximum x-google-start-bitrate (kbps).
    /// 1 Mbps is a reasonable ceiling that prevents BWE from starting too aggressively.
    const MAX_START_BITRATE_KBPS: u32 = 1000;

    /// Compute the x-google-start-bitrate value for SDP munging.
    ///
    /// Returns min(90% of target, 1 Mbps). Returns None if no target bitrate is set
    /// (initial offer before track publish) or if the target is too low.
    fn compute_start_bitrate_kbps(target_bps: Option<u64>) -> Option<u32> {
        let target_bps = target_bps?;
        let target_kbps = (target_bps / 1000) as u32;

        if target_kbps == 0 || target_kbps < 300 {
            return None;
        }

        // Use 90% of target bitrate as start bitrate, capped at 1 Mbps
        let start_kbps = (target_kbps as f64 * 0.9).round() as u32;
        Some(start_kbps.min(target_kbps).min(Self::MAX_START_BITRATE_KBPS))
    }

    /// Munge SDP to change a=inactive to a=recvonly for RTP media m-lines in single PC mode.
    /// This is needed because WebRTC can generate inactive direction even when transceivers
    /// were configured as recvonly.
    ///
    /// We intentionally limit this to RTP m-sections, so non-RTP sections (for example
    /// data-channel `m=application` sections) are not rewritten.
    fn munge_inactive_to_recvonly_for_media(sdp: &str) -> String {
        // Detect what line ending the original SDP uses
        let uses_crlf = sdp.contains("\r\n");
        let eol = if uses_crlf { "\r\n" } else { "\n" };

        let lines: Vec<&str> =
            if uses_crlf { sdp.split("\r\n").collect() } else { sdp.split('\n').collect() };

        let mut out: Vec<String> = Vec::with_capacity(lines.len());
        let mut in_rtp_media_section = false;

        for line in lines {
            let l = line.trim();

            // Track whether the current m-section is RTP-based.
            if l.starts_with("m=") {
                // Example RTP m-line:
                //   m=audio 9 UDP/TLS/RTP/SAVPF 111
                // Example data channel m-line:
                //   m=application 9 UDP/DTLS/SCTP webrtc-datachannel
                in_rtp_media_section = l.contains("RTP/");
            }

            // Change inactive to recvonly for RTP media m-sections.
            if in_rtp_media_section && l == "a=inactive" {
                out.push("a=recvonly".to_string());
            } else {
                out.push(line.to_string());
            }
        }

        let mut munged = out.join(eol);
        if !munged.ends_with(eol) {
            munged.push_str(eol);
        }
        munged
    }

    /// Munge SDP to add stereo=1 to opus audio fmtp lines for single PC mode
    /// As per the doc: "In single peer connection mode, the receiver sends the offer,
    /// hence does not know if the sender will send stereo. Therefore, stereo=1 is not set
    /// in the offer. Always set stereo=1 in the offer - This method works."
    fn munge_stereo_for_audio(sdp: &str) -> String {
        // Detect what line ending the original SDP uses
        let uses_crlf = sdp.contains("\r\n");
        let eol = if uses_crlf { "\r\n" } else { "\n" };

        // Split preserving the intended line ending style
        let lines: Vec<&str> =
            if uses_crlf { sdp.split("\r\n").collect() } else { sdp.split('\n').collect() };

        // Find opus payload type (usually 111, but be flexible)
        let mut opus_pts: Vec<&str> = Vec::new();
        for line in &lines {
            let l = line.trim();
            if let Some(rest) = l.strip_prefix("a=rtpmap:") {
                let mut it = rest.split_whitespace();
                let pt = it.next().unwrap_or("");
                let codec = it.next().unwrap_or("");
                // Match opus/48000/2 (stereo opus)
                if codec.starts_with("opus/48000") && !pt.is_empty() {
                    opus_pts.push(pt);
                }
            }
        }

        if opus_pts.is_empty() {
            return sdp.to_string();
        }

        // Rewrite fmtp lines to add stereo=1 if not present
        let mut out: Vec<String> = Vec::with_capacity(lines.len());
        for line in lines {
            let mut rewritten = line.to_string();

            for pt in &opus_pts {
                let prefix = format!("a=fmtp:{pt} ");
                if rewritten.starts_with(&prefix) {
                    // Check if stereo= already exists
                    if !rewritten.contains("stereo=") {
                        // Append stereo=1
                        rewritten.push_str(";stereo=1");
                    }
                    break;
                }
            }

            out.push(rewritten);
        }

        // Re-join using same EOL
        let mut munged = out.join(eol);
        if !munged.ends_with(eol) {
            munged.push_str(eol);
        }
        munged
    }

    /// Check if a codec string represents a video codec that should get start bitrate hint.
    fn is_video_codec(codec: &str) -> bool {
        codec.starts_with("VP8/90000")
            || codec.starts_with("VP9/90000")
            || codec.starts_with("AV1/90000")
            || codec.starts_with("H264/90000")
            || codec.starts_with("H265/90000")
    }

    fn munge_x_google_start_bitrate(sdp: &str, start_bitrate_kbps: u32) -> String {
        // Detect what line ending the original SDP uses
        let uses_crlf = sdp.contains("\r\n");
        let eol = if uses_crlf { "\r\n" } else { "\n" };

        // Split preserving the intended line ending style
        let lines: Vec<&str> =
            if uses_crlf { sdp.split("\r\n").collect() } else { sdp.split('\n').collect() };

        // 1) Find all video codec payload types (VP8, VP9, AV1, H264, H265)
        let mut target_pts: Vec<&str> = Vec::new();
        for line in &lines {
            let l = line.trim();
            if let Some(rest) = l.strip_prefix("a=rtpmap:") {
                let mut it = rest.split_whitespace();
                let pt = it.next().unwrap_or("");
                let codec = it.next().unwrap_or("");
                if Self::is_video_codec(codec) && !pt.is_empty() {
                    target_pts.push(pt);
                }
            }
        }
        if target_pts.is_empty() {
            return sdp.to_string();
        }

        // 2) Rewrite fmtp lines (minimal mutation)
        let mut out: Vec<String> = Vec::with_capacity(lines.len());
        for line in lines {
            let mut rewritten = line.to_string();

            for pt in &target_pts {
                let prefix = format!("a=fmtp:{pt} ");
                if rewritten.starts_with(&prefix) {
                    // Replace if present; append if not present
                    if let Some(pos) = rewritten.find("x-google-start-bitrate=") {
                        // replace existing value up to next ';' or end
                        let after = &rewritten[pos..];
                        let end =
                            after.find(';').map(|i| pos + i).unwrap_or_else(|| rewritten.len());
                        rewritten.replace_range(
                            pos..end,
                            &format!("x-google-start-bitrate={start_bitrate_kbps}"),
                        );
                    } else {
                        rewritten
                            .push_str(&format!(";x-google-start-bitrate={start_bitrate_kbps}"));
                    }
                    break;
                }
            }

            out.push(rewritten);
        }

        // 3) For video codecs that don't already have fmtp lines, create new ones
        // with x-google-start-bitrate. This handles cases where the browser/WebRTC
        // didn't generate an fmtp line for a particular payload type (e.g., VP8
        // typically has no fmtp, while H.264/VP9-SVC/AV1 usually do).
        let pts_with_fmtp: std::collections::HashSet<String> = out
            .iter()
            .filter_map(|line| {
                let l = line.trim();
                if let Some(rest) = l.strip_prefix("a=fmtp:") {
                    rest.split_whitespace().next().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();

        // Find rtpmap lines and insert fmtp after them for video codecs without existing fmtp
        let mut final_out: Vec<String> = Vec::with_capacity(out.len() + target_pts.len());
        for line in out.iter() {
            final_out.push(line.clone());

            // Check if this is an rtpmap line for a video codec without an existing fmtp line
            let l = line.trim();
            if let Some(rest) = l.strip_prefix("a=rtpmap:") {
                let mut it = rest.split_whitespace();
                let pt = it.next().unwrap_or("");
                let codec = it.next().unwrap_or("");
                if Self::is_video_codec(codec) && !pt.is_empty() && !pts_with_fmtp.contains(pt) {
                    // Create fmtp line with x-google-start-bitrate
                    let fmtp_line =
                        format!("a=fmtp:{pt} x-google-start-bitrate={start_bitrate_kbps}");
                    log::debug!("Creating fmtp line for {} (pt={}): {}", codec, pt, fmtp_line);
                    final_out.push(fmtp_line);
                }
            }
        }

        // Re-join using same EOL, and ensure trailing EOL (some parsers are picky)
        let mut munged = final_out.join(eol);
        if !munged.ends_with(eol) {
            munged.push_str(eol);
        }
        munged
    }

    pub async fn create_and_send_offer(&self, options: OfferOptions) -> EngineResult<()> {
        let mut inner = self.inner.lock().await;
        if options.ice_restart {
            inner.restarting_ice = true;
        }

        if inner.pending_initial_offer.is_some() {
            inner.renegotiate = true;
            return Ok(());
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
        let mut sdp = offer.to_string();

        if inner.single_pc_mode {
            // Fix inactive media m-lines to recvonly for single PC mode.
            // WebRTC can generate a=inactive even when transceivers are recvonly.
            let recvonly_munged = Self::munge_inactive_to_recvonly_for_media(&sdp);
            if recvonly_munged != sdp {
                match SessionDescription::parse(&recvonly_munged, offer.sdp_type()) {
                    Ok(parsed) => {
                        offer = parsed;
                        sdp = recvonly_munged;
                    }
                    Err(e) => {
                        log::warn!("Failed to parse recvonly-munged SDP: {e}");
                    }
                }
            }

            // Apply stereo munging for single PC mode
            let stereo_munged = Self::munge_stereo_for_audio(&sdp);
            if stereo_munged != sdp {
                match SessionDescription::parse(&stereo_munged, offer.sdp_type()) {
                    Ok(parsed) => {
                        offer = parsed;
                        sdp = stereo_munged;
                    }
                    Err(e) => {
                        log::warn!("Failed to parse stereo-munged SDP, using original: {e}");
                    }
                }
            }
        }

        // Apply x-google-start-bitrate for all video codecs to improve initial quality.
        // Uses min(90% of target, 1 Mbps) to prevent BWE from starting too aggressively.
        let has_video = sdp.contains(" VP8/90000")
            || sdp.contains(" VP9/90000")
            || sdp.contains(" AV1/90000")
            || sdp.contains(" H264/90000")
            || sdp.contains(" H265/90000");
        if has_video {
            if let Some(start_kbps) = Self::compute_start_bitrate_kbps(inner.max_send_bitrate_bps) {
                log::info!(
                    "Applying x-google-start-bitrate={} kbps (target_bps={:?})",
                    start_kbps,
                    inner.max_send_bitrate_bps
                );

                let munged = Self::munge_x_google_start_bitrate(&sdp, start_kbps);
                if munged != sdp {
                    log::debug!("SDP munged successfully for video codec");
                    match SessionDescription::parse(&munged, offer.sdp_type()) {
                        Ok(parsed) => offer = parsed,
                        Err(e) => log::warn!(
                            "Failed to parse munged SDP, falling back to original offer: {e}"
                        ),
                    }
                } else {
                    log::debug!("SDP munging produced no changes");
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

#[cfg(test)]
mod tests {
    use super::PeerTransport;

    /// Reproduces the publisher-transport self-deadlock.
    ///
    /// `PeerTransport::set_remote_description` locks `self.inner` and then, when
    /// `inner.renegotiate` is set, calls `create_and_send_offer` *while still holding that
    /// guard*. `create_and_send_offer`'s first statement re-locks the same
    /// `tokio::sync::Mutex`, which is not reentrant, so the task waits forever for a lock it
    /// already holds.
    ///
    /// This test drives the production-reachable `HaveLocalOffer` sequence that sets
    /// `renegotiate = true`:
    ///
    ///  1. The publisher creates and sends an ordinary offer, entering `HaveLocalOffer`.
    ///  2. Before the answer is applied, another publish/unpublish negotiation is requested.
    ///     `create_and_send_offer` sees `HaveLocalOffer` and sets `renegotiate = true`.
    ///  3. The answer reaches the transport. Because `renegotiate == true`,
    ///     `set_remote_description` re-enters `create_and_send_offer`; holding the inner lock
    ///     across that call would deadlock.
    ///
    /// The `timeout` distinguishes a deadlock (the future never resolves) from success. On the
    /// buggy code this test fails via the timeout assertion; with the lock released before the
    /// nested call it passes.
    #[tokio::test]
    async fn renegotiation_does_not_deadlock() {
        use std::{
            sync::{Arc, Mutex},
            time::Duration,
        };

        use libwebrtc::prelude::*;
        use livekit_protocol as proto;

        let factory = PeerConnectionFactory::default();
        let config = RtcConfiguration {
            ice_servers: vec![],
            continual_gathering_policy: ContinualGatheringPolicy::GatherOnce,
            ice_transport_type: IceTransportsType::All,
        };

        let alice_pc = factory.create_peer_connection(config.clone()).unwrap();
        let bob_pc = factory.create_peer_connection(config).unwrap();

        // Give the publisher an m-line so the offer/answer exchange is non-trivial.
        let _dc = alice_pc.create_data_channel("repro", DataChannelInit::default()).unwrap();

        let transport = PeerTransport::new(
            alice_pc,
            proto::SignalTarget::Publisher,
            /* single_pc_mode= */ true,
        );

        let offers = Arc::new(Mutex::new(Vec::new()));
        let emitted_offers = offers.clone();
        transport.on_offer(Some(Box::new(move |offer| {
            emitted_offers.lock().expect("offers lock poisoned").push(offer);
        })));

        // 1. Send an ordinary publisher offer, putting Alice in `HaveLocalOffer`.
        transport.create_and_send_offer(OfferOptions::default()).await.unwrap();
        assert_eq!(transport.peer_connection().signaling_state(), SignalingState::HaveLocalOffer);

        let offer = offers
            .lock()
            .expect("offers lock poisoned")
            .first()
            .cloned()
            .expect("first offer was not emitted");

        // Bob prepares the answer, but it has not reached Alice yet.
        bob_pc.set_remote_description(offer).await.unwrap();
        let answer = bob_pc.create_answer(AnswerOptions::default()).await.unwrap();
        bob_pc.set_local_description(answer.clone()).await.unwrap();

        // 2. Model another publish/unpublish negotiation during the offer/answer RTT. Because
        //    Alice is still in `HaveLocalOffer`, this only sets `renegotiate = true`.
        transport.create_and_send_offer(OfferOptions::default()).await.unwrap();
        assert_eq!(offers.lock().expect("offers lock poisoned").len(), 1);

        // 3. Applying the answer must complete and emit the deferred follow-up offer.
        let res =
            tokio::time::timeout(Duration::from_secs(10), transport.set_remote_description(answer))
                .await;

        assert!(
            res.is_ok(),
            "DEADLOCK: set_remote_description re-locked the transport's `inner` AsyncMutex while \
             renegotiating (peer_transport.rs: lock at set_remote_description, re-lock at \
             create_and_send_offer)"
        );
        res.unwrap().expect("set_remote_description returned an error");

        assert_eq!(
            offers.lock().expect("offers lock poisoned").len(),
            2,
            "the deferred renegotiation should emit a follow-up offer"
        );
        assert_eq!(transport.peer_connection().signaling_state(), SignalingState::HaveLocalOffer);
    }

    /// The negotiation generation must advance exactly when a remote description is applied,
    /// because the resume path treats that advance as proof that a transport re-established
    /// against the node it landed on. If it failed to bump, a genuine recovery would be
    /// rejected and escalate to an unnecessary full reconnect; if it bumped without a real
    /// negotiation, a dead transport would be accepted as recovered.
    #[tokio::test]
    async fn negotiation_generation_advances_on_applied_remote_description() {
        use libwebrtc::prelude::*;
        use livekit_protocol as proto;

        let factory = PeerConnectionFactory::default();
        let config = RtcConfiguration {
            ice_servers: vec![],
            continual_gathering_policy: ContinualGatheringPolicy::GatherOnce,
            ice_transport_type: IceTransportsType::All,
        };

        let alice_pc = factory.create_peer_connection(config.clone()).unwrap();
        let bob_pc = factory.create_peer_connection(config).unwrap();
        let _dc = alice_pc.create_data_channel("gen", DataChannelInit::default()).unwrap();

        let transport = PeerTransport::new(
            alice_pc,
            proto::SignalTarget::Publisher,
            /* single_pc_mode= */ true,
        );

        let offers = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let emitted = offers.clone();
        transport.on_offer(Some(Box::new(move |offer| {
            emitted.lock().expect("offers lock poisoned").push(offer);
        })));

        assert_eq!(transport.negotiation_generation(), 0);

        // Sending an offer is not by itself a completed negotiation — nothing has come back
        // from the far end yet, so there is no evidence of a live path.
        transport.create_and_send_offer(OfferOptions::default()).await.unwrap();
        assert_eq!(
            transport.negotiation_generation(),
            0,
            "an unanswered offer must not count as a completed negotiation"
        );

        let offer = offers
            .lock()
            .expect("offers lock poisoned")
            .first()
            .cloned()
            .expect("offer was not emitted");

        bob_pc.set_remote_description(offer).await.unwrap();
        let answer = bob_pc.create_answer(AnswerOptions::default()).await.unwrap();
        bob_pc.set_local_description(answer.clone()).await.unwrap();

        // Applying the answer completes the round-trip.
        transport.set_remote_description(answer).await.unwrap();
        assert_eq!(transport.negotiation_generation(), 1);
    }

    /// The disconnect generation records leaving `Connected` durably, so a resume polling
    /// afterwards can still tell that the transport broke — the failure it must not miss is
    /// a transport that dropped and returned to `Connected` between two polls.
    #[test]
    fn disconnect_generation_records_leaving_connected() {
        use libwebrtc::prelude::*;
        use livekit_protocol as proto;

        let factory = PeerConnectionFactory::default();
        let pc = factory
            .create_peer_connection(RtcConfiguration {
                ice_servers: vec![],
                continual_gathering_policy: ContinualGatheringPolicy::GatherOnce,
                ice_transport_type: IceTransportsType::All,
            })
            .unwrap();

        let transport = PeerTransport::new(
            pc,
            proto::SignalTarget::Subscriber,
            /* single_pc_mode= */ false,
        );

        assert_eq!(transport.disconnect_generation(), 0);

        transport.note_connection_state(PeerConnectionState::Connected);
        assert_eq!(transport.disconnect_generation(), 0, "staying Connected is not a disconnect");

        transport.note_connection_state(PeerConnectionState::Disconnected);
        assert_eq!(transport.disconnect_generation(), 1);

        // Returning to Connected leaves the record standing: the resume must still be able
        // to see that the transport broke.
        transport.note_connection_state(PeerConnectionState::Connected);
        assert_eq!(transport.disconnect_generation(), 1);

        transport.note_connection_state(PeerConnectionState::Failed);
        assert_eq!(transport.disconnect_generation(), 2);
    }

    /// A resume marks the subscriber as awaiting a fresh offer, but the SFU only re-offers
    /// when the resume moved the participant to another node. After the far more common
    /// signal-only resume no offer arrives, so nothing in `set_remote_description` ever
    /// clears the flag — and every subsequent remote candidate is buffered instead of
    /// applied, leaving the transport unable to adopt any new network path the server
    /// proposes for the rest of the session.
    ///
    /// `finish_restarting_ice` closes that window explicitly. This test drives the
    /// no-re-offer sequence: mark, queue a candidate, finish, and assert the queue drained
    /// and later candidates go straight through.
    #[tokio::test]
    async fn finishing_ice_restart_without_a_new_offer_resumes_applying_candidates() {
        use libwebrtc::prelude::*;
        use livekit_protocol as proto;

        let factory = PeerConnectionFactory::default();
        let config = RtcConfiguration {
            ice_servers: vec![],
            continual_gathering_policy: ContinualGatheringPolicy::GatherOnce,
            ice_transport_type: IceTransportsType::All,
        };

        // Stand up a subscriber-shaped transport that has a remote description, which is the
        // state a resume finds it in.
        let alice_pc = factory.create_peer_connection(config.clone()).unwrap();
        let bob_pc = factory.create_peer_connection(config).unwrap();
        let _dc = bob_pc.create_data_channel("sub", DataChannelInit::default()).unwrap();

        let transport = PeerTransport::new(
            alice_pc,
            proto::SignalTarget::Subscriber,
            /* single_pc_mode= */ false,
        );

        // Bob (standing in for the SFU) offers; Alice answers. Alice now has a remote
        // description, so candidates would normally apply immediately.
        let offer = bob_pc.create_offer(OfferOptions::default()).await.unwrap();
        bob_pc.set_local_description(offer.clone()).await.unwrap();
        transport.create_anwser(offer, AnswerOptions::default()).await.unwrap();
        assert!(transport.peer_connection().current_remote_description().is_some());

        let candidate = |port| {
            IceCandidate::parse(
                "0",
                0,
                &format!("candidate:1 1 UDP 2130706431 192.168.1.1 {port} typ host"),
            )
            .expect("test candidate should parse")
        };

        // A resume opens the window; candidates must now queue rather than be applied to a
        // generation that may be on its way out.
        transport.mark_restarting_ice().await;
        transport.add_ice_candidate(candidate(50000)).await.unwrap();
        assert_eq!(
            transport.inner.lock().await.pending_candidates.len(),
            1,
            "candidates must queue while the transport awaits a fresh offer"
        );

        // The SFU never re-offers -- the signal-only resume case. Closing the window must
        // drain what queued behind it.
        transport.finish_restarting_ice().await.unwrap();
        assert!(
            !transport.inner.lock().await.restarting_ice,
            "the window must not outlive the resume that opened it"
        );
        assert!(
            transport.inner.lock().await.pending_candidates.is_empty(),
            "queued candidates must be applied when the window closes"
        );

        // And the transport must be back to applying candidates as they arrive, so it can
        // still adopt new network paths the server proposes.
        transport.add_ice_candidate(candidate(50001)).await.unwrap();
        assert!(
            transport.inner.lock().await.pending_candidates.is_empty(),
            "later candidates must be applied directly, not buffered"
        );
    }

    /// Closing a window that was never opened must not disturb a transport that is legitimately
    /// queueing candidates because it has no remote description yet.
    #[tokio::test]
    async fn finishing_ice_restart_is_a_noop_when_not_restarting() {
        use libwebrtc::prelude::*;
        use livekit_protocol as proto;

        let factory = PeerConnectionFactory::default();
        let pc = factory
            .create_peer_connection(RtcConfiguration {
                ice_servers: vec![],
                continual_gathering_policy: ContinualGatheringPolicy::GatherOnce,
                ice_transport_type: IceTransportsType::All,
            })
            .unwrap();

        let transport = PeerTransport::new(
            pc,
            proto::SignalTarget::Subscriber,
            /* single_pc_mode= */ false,
        );

        // No remote description yet, so this candidate queues for that reason, not because of
        // an ICE restart.
        let candidate =
            IceCandidate::parse("0", 0, "candidate:1 1 UDP 2130706431 192.168.1.1 50000 typ host")
                .expect("test candidate should parse");
        transport.add_ice_candidate(candidate).await.unwrap();
        assert_eq!(transport.inner.lock().await.pending_candidates.len(), 1);

        transport.finish_restarting_ice().await.unwrap();
        assert_eq!(
            transport.inner.lock().await.pending_candidates.len(),
            1,
            "candidates awaiting a first remote description must stay queued"
        );
    }

    /// The window must close even when the resume did nothing else at all -- a publisher
    /// offer that fails immediately after the window opens still has to leave the subscriber
    /// applying candidates. Closing twice must also be harmless, since the SFU's offer may
    /// already have closed the window before the resume gets round to it.
    #[tokio::test]
    async fn finishing_ice_restart_is_unconditional_and_idempotent() {
        use libwebrtc::prelude::*;
        use livekit_protocol as proto;

        let factory = PeerConnectionFactory::default();
        let config = RtcConfiguration {
            ice_servers: vec![],
            continual_gathering_policy: ContinualGatheringPolicy::GatherOnce,
            ice_transport_type: IceTransportsType::All,
        };

        let alice_pc = factory.create_peer_connection(config.clone()).unwrap();
        let bob_pc = factory.create_peer_connection(config).unwrap();
        let _dc = bob_pc.create_data_channel("sub", DataChannelInit::default()).unwrap();

        let transport = PeerTransport::new(
            alice_pc,
            proto::SignalTarget::Subscriber,
            /* single_pc_mode= */ false,
        );

        let offer = bob_pc.create_offer(OfferOptions::default()).await.unwrap();
        bob_pc.set_local_description(offer.clone()).await.unwrap();
        transport.create_anwser(offer, AnswerOptions::default()).await.unwrap();

        // Open the window, then close it with nothing having happened in between.
        transport.mark_restarting_ice().await;
        transport.finish_restarting_ice().await.unwrap();
        assert!(!transport.inner.lock().await.restarting_ice);

        // Closing again is a no-op rather than an error.
        transport.finish_restarting_ice().await.unwrap();
        assert!(!transport.inner.lock().await.restarting_ice);

        // The transport is still usable: candidates apply rather than queue.
        let candidate =
            IceCandidate::parse("0", 0, "candidate:1 1 UDP 2130706431 192.168.1.1 50000 typ host")
                .expect("test candidate should parse");
        transport.add_ice_candidate(candidate).await.unwrap();
        assert!(transport.inner.lock().await.pending_candidates.is_empty());
    }

    #[test]
    fn no_video_codec_is_noop() {
        // Audio-only SDP should not be modified
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\n\
a=rtpmap:111 opus/48000/2\n\
a=fmtp:111 minptime=10;useinbandfec=1\n";
        let out = PeerTransport::munge_x_google_start_bitrate(sdp, 3200);
        assert_eq!(out, sdp, "should not change SDP if no video codec present");
    }

    #[test]
    fn vp8_with_fmtp_appends_start_bitrate() {
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=video 9 UDP/TLS/RTP/SAVPF 96\n\
a=rtpmap:96 VP8/90000\n\
a=fmtp:96 some=param\n";
        let out = PeerTransport::munge_x_google_start_bitrate(sdp, 3200);
        assert!(
            out.contains("a=fmtp:96 some=param;x-google-start-bitrate=3200\n"),
            "VP8 fmtp should get x-google-start-bitrate appended"
        );
    }

    #[test]
    fn h264_with_fmtp_appends_start_bitrate() {
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=video 9 UDP/TLS/RTP/SAVPF 102\n\
a=rtpmap:102 H264/90000\n\
a=fmtp:102 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f\n";
        let out = PeerTransport::munge_x_google_start_bitrate(sdp, 4000);
        assert!(
            out.contains("a=fmtp:102 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42001f;x-google-start-bitrate=4000\n"),
            "H264 fmtp should get x-google-start-bitrate appended"
        );
    }

    #[test]
    fn vp9_with_fmtp_appends_start_bitrate_and_preserves_lf_and_trailing_eol() {
        // LF-only SDP, ends with \n already
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=video 9 UDP/TLS/RTP/SAVPF 98\n\
a=rtpmap:98 VP9/90000\n\
a=fmtp:98 profile-id=0\n";
        let out = PeerTransport::munge_x_google_start_bitrate(sdp, 3200);

        assert!(out.contains("a=fmtp:98 profile-id=0;x-google-start-bitrate=3200\n"));
        assert!(!out.contains("\r\n"), "should preserve LF-only line endings");
        assert!(out.ends_with('\n'), "should end with a trailing LF");
    }

    #[test]
    fn av1_with_fmtp_replaces_existing_start_bitrate_value() {
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=video 9 UDP/TLS/RTP/SAVPF 104\n\
a=rtpmap:104 AV1/90000\n\
a=fmtp:104 x-google-start-bitrate=1000;foo=bar\n";
        let out = PeerTransport::munge_x_google_start_bitrate(sdp, 2500);
        assert!(
            out.contains("a=fmtp:104 x-google-start-bitrate=2500;foo=bar\n"),
            "should replace existing x-google-start-bitrate value and keep other params"
        );
        assert!(!out.contains("x-google-start-bitrate=1000"), "old bitrate value should be gone");
    }

    #[test]
    fn vp9_without_fmtp_line_creates_one() {
        // VP9 rtpmap exists, but no fmtp: function creates a new fmtp line with x-google-start-bitrate.
        // This ensures all video codecs (including those like VP8 that typically lack fmtp) get the hint.
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=video 9 UDP/TLS/RTP/SAVPF 98\n\
a=rtpmap:98 VP9/90000\n";
        let out = PeerTransport::munge_x_google_start_bitrate(sdp, 3200);
        let expected = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=video 9 UDP/TLS/RTP/SAVPF 98\n\
a=rtpmap:98 VP9/90000\n\
a=fmtp:98 x-google-start-bitrate=3200\n";
        assert_eq!(
            out, expected,
            "should create fmtp line with x-google-start-bitrate for video codec without fmtp"
        );
    }

    #[test]
    fn preserves_crlf_and_adds_trailing_crlf_if_missing() {
        // CRLF SDP without trailing CRLF at the end (common edge)
        let sdp = "v=0\r\n\
o=- 0 0 IN IP4 127.0.0.1\r\n\
s=-\r\n\
t=0 0\r\n\
m=video 9 UDP/TLS/RTP/SAVPF 98\r\n\
a=rtpmap:98 VP9/90000\r\n\
a=fmtp:98 profile-id=0"; // <- no final \r\n
        let out = PeerTransport::munge_x_google_start_bitrate(sdp, 3200);
        assert!(out.contains("a=fmtp:98 profile-id=0;x-google-start-bitrate=3200\r\n"));
        assert!(out.contains("\r\n"), "should keep CRLF line endings");
        assert!(out.ends_with("\r\n"), "should ensure trailing CRLF");
        assert!(!out.contains("\n") || out.contains("\r\n"), "should not introduce lone LF");
    }

    #[test]
    fn multiple_video_codecs_all_get_munged() {
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=video 9 UDP/TLS/RTP/SAVPF 96 98 104\n\
a=rtpmap:96 VP8/90000\n\
a=rtpmap:98 VP9/90000\n\
a=rtpmap:104 AV1/90000\n\
a=fmtp:96 foo=bar\n\
a=fmtp:98 profile-id=0\n\
a=fmtp:104 x-google-start-bitrate=1111;baz=qux\n";
        let out = PeerTransport::munge_x_google_start_bitrate(sdp, 2222);
        // VP8 fmtp should get appended
        assert!(out.contains("a=fmtp:96 foo=bar;x-google-start-bitrate=2222\n"));
        // VP9 fmtp should get appended
        assert!(out.contains("a=fmtp:98 profile-id=0;x-google-start-bitrate=2222\n"));
        // AV1 fmtp should get replaced
        assert!(out.contains("a=fmtp:104 x-google-start-bitrate=2222;baz=qux\n"));
        assert!(!out.contains("a=fmtp:104 x-google-start-bitrate=1111"));
    }

    #[test]
    fn all_video_codecs_get_fmtp_with_start_bitrate() {
        // Mixed scenario: some codecs have fmtp, some don't
        // All video codecs should end up with exactly one fmtp line containing x-google-start-bitrate
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=video 9 UDP/TLS/RTP/SAVPF 96 97 98 99 100\n\
a=rtpmap:96 VP8/90000\n\
a=rtpmap:97 VP9/90000\n\
a=fmtp:97 profile-id=0\n\
a=rtpmap:98 H264/90000\n\
a=fmtp:98 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f\n\
a=rtpmap:99 AV1/90000\n\
a=rtpmap:100 H265/90000\n\
a=fmtp:100 profile-id=1\n";
        let out = PeerTransport::munge_x_google_start_bitrate(sdp, 900);

        // VP8 (96): had no fmtp, should get new one
        assert!(
            out.contains("a=fmtp:96 x-google-start-bitrate=900\n"),
            "VP8 should get new fmtp line"
        );
        assert_eq!(out.matches("a=fmtp:96 ").count(), 1, "VP8 should have exactly one fmtp line");

        // VP9 (97): had fmtp, should get bitrate appended
        assert!(
            out.contains("a=fmtp:97 profile-id=0;x-google-start-bitrate=900\n"),
            "VP9 should have bitrate appended to existing fmtp"
        );
        assert_eq!(out.matches("a=fmtp:97 ").count(), 1, "VP9 should have exactly one fmtp line");

        // H264 (98): had fmtp, should get bitrate appended
        assert!(
            out.contains("a=fmtp:98 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f;x-google-start-bitrate=900\n"),
            "H264 should have bitrate appended to existing fmtp"
        );
        assert_eq!(out.matches("a=fmtp:98 ").count(), 1, "H264 should have exactly one fmtp line");

        // AV1 (99): had no fmtp, should get new one
        assert!(
            out.contains("a=fmtp:99 x-google-start-bitrate=900\n"),
            "AV1 should get new fmtp line"
        );
        assert_eq!(out.matches("a=fmtp:99 ").count(), 1, "AV1 should have exactly one fmtp line");

        // H265 (100): had fmtp, should get bitrate appended
        assert!(
            out.contains("a=fmtp:100 profile-id=1;x-google-start-bitrate=900\n"),
            "H265 should have bitrate appended to existing fmtp"
        );
        assert_eq!(out.matches("a=fmtp:100 ").count(), 1, "H265 should have exactly one fmtp line");

        // Total: 5 video codecs, 5 fmtp lines with x-google-start-bitrate
        assert_eq!(
            out.matches("x-google-start-bitrate=900").count(),
            5,
            "all 5 video codecs should have x-google-start-bitrate"
        );
    }

    #[test]
    fn does_not_duplicate_start_bitrate_when_already_present_no_semicolon_following() {
        // Existing x-google-start-bitrate at end of line (no trailing ';')
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=video 9 UDP/TLS/RTP/SAVPF 98\n\
a=rtpmap:98 VP9/90000\n\
a=fmtp:98 profile-id=0;x-google-start-bitrate=1000\n";
        let out = PeerTransport::munge_x_google_start_bitrate(sdp, 3000);
        assert!(out.contains("a=fmtp:98 profile-id=0;x-google-start-bitrate=3000\n"));
        assert!(!out.contains("x-google-start-bitrate=1000"));
        // ensure only one occurrence
        assert_eq!(out.matches("x-google-start-bitrate=").count(), 1);
    }

    #[test]
    fn inactive_media_is_munged_to_recvonly_for_all_rtp_sections() {
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\n\
a=inactive\n\
a=rtpmap:111 opus/48000/2\n\
m=video 9 UDP/TLS/RTP/SAVPF 96\n\
a=inactive\n\
m=text 9 UDP/TLS/RTP/SAVPF 98\n\
a=inactive\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\n\
a=inactive\n";
        let out = PeerTransport::munge_inactive_to_recvonly_for_media(sdp);
        assert!(out.contains("m=audio 9 UDP/TLS/RTP/SAVPF 111\na=recvonly\n"));
        assert!(out.contains("m=text 9 UDP/TLS/RTP/SAVPF 98\na=recvonly\n"));
        assert_eq!(out.matches("a=recvonly").count(), 4);
        assert_eq!(out.matches("a=inactive").count(), 0);
    }

    #[test]
    fn inactive_application_section_is_not_munged() {
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\n\
a=inactive\n\
m=application 9 UDP/DTLS/SCTP webrtc-datachannel\n\
a=inactive\n";
        let out = PeerTransport::munge_inactive_to_recvonly_for_media(sdp);
        assert!(out.contains("m=audio 9 UDP/TLS/RTP/SAVPF 111\na=recvonly\n"));
        assert!(out.contains("m=application 9 UDP/DTLS/SCTP webrtc-datachannel\na=inactive\n"));
    }

    #[test]
    fn stereo_is_added_for_opus_fmtp_only_once() {
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111 0\n\
a=rtpmap:111 opus/48000/2\n\
a=rtpmap:0 PCMU/8000\n\
a=fmtp:111 minptime=10;useinbandfec=1\n\
a=fmtp:0 foo=bar\n";
        let out = PeerTransport::munge_stereo_for_audio(sdp);
        assert!(out.contains("a=fmtp:111 minptime=10;useinbandfec=1;stereo=1\n"));
        assert!(out.contains("a=fmtp:0 foo=bar\n"));
        assert_eq!(out.matches("stereo=1").count(), 1);
    }

    #[test]
    fn stereo_munging_is_idempotent_when_stereo_already_present() {
        let sdp = "v=0\n\
o=- 0 0 IN IP4 127.0.0.1\n\
s=-\n\
t=0 0\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111\n\
a=rtpmap:111 opus/48000/2\n\
a=fmtp:111 minptime=10;stereo=1\n";
        let out = PeerTransport::munge_stereo_for_audio(sdp);
        assert_eq!(out.matches("stereo=1").count(), 1);
    }
}
