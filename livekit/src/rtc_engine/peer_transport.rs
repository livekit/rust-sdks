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
    time::Duration,
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
    // Whether the target above belongs to a screen share, which is exempt from the
    // start bitrate cap.
    max_send_bitrate_is_screen_share: bool,
    // How long this peer connection took to set up, set once the initial connect succeeds.
    // `compute_start_bitrate_kbps` lowers the hint for slow connections.
    connection_setup_time: Option<Duration>,
    // Whether an offer carrying `x-google-start-bitrate` has been accepted locally. The
    // hint is written once per peer connection; see `compute_start_bitrate_kbps`.
    start_bitrate_applied: bool,
    pending_initial_offer: Option<MungedOffer>,
}

/// An offer after local SDP munging, see [`PeerTransport::munge_offer`].
struct MungedOffer {
    offer: SessionDescription,
    /// Whether the offer carries the one-shot `x-google-start-bitrate` hint.
    has_start_bitrate: bool,
}

pub struct PeerTransport {
    signal_target: proto::SignalTarget,
    peer_connection: PeerConnection,
    on_offer_handler: Mutex<Option<OnOfferCreated>>,
    inner: Arc<AsyncMutex<TransportInner>>,

    /// Counts entries into `Connected`; see [`Self::note_connection_state`].
    connected_generation: AtomicU32,

    /// Counts exits from `Connected`; see [`Self::note_connection_state`].
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
                max_send_bitrate_is_screen_share: false,
                connection_setup_time: None,
                start_bitrate_applied: false,
                pending_initial_offer: None,
            })),
            connected_generation: AtomicU32::new(0),
            disconnect_generation: AtomicU32::new(0),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.peer_connection.connection_state() == PeerConnectionState::Connected
    }

    pub fn connected_generation(&self) -> u32 {
        self.connected_generation.load(Ordering::Acquire)
    }

    pub fn disconnect_generation(&self) -> u32 {
        self.disconnect_generation.load(Ordering::Acquire)
    }

    /// Record a connection-state transition, so a later observer can tell what the transport
    /// has *done* rather than only what it currently reports.
    ///
    /// Both counters exist because `PeerConnectionState` is a level, and a level cannot
    /// distinguish a transport that recovered from one whose far end vanished: ICE keeps
    /// reporting `Connected` until its receiving timeout, and only reaches `Failed` after
    /// consent expiry tens of seconds later. Comparing these against a snapshot taken before
    /// a resume gives that history. Called for every `RtcEvent::ConnectionChange`, so a
    /// transport that drops and returns between two polls is still visible as having dropped.
    pub fn note_connection_state(&self, state: PeerConnectionState) {
        if state == PeerConnectionState::Connected {
            self.connected_generation.fetch_add(1, Ordering::AcqRel);
        } else {
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

        if let Some(pending) = inner.pending_initial_offer.take() {
            self.peer_connection.set_local_description(pending.offer).await?;
            // Only consume the one-shot hint once the offer carrying it is accepted locally.
            if pending.has_start_bitrate {
                inner.start_bitrate_applied = true;
            }
        }

        self.peer_connection.set_remote_description(remote_description).await?;

        // Apply every queued candidate, even if one fails. A candidate can be rejected on its
        // own merits — a malformed or stale line from the server — and that must not abort the
        // replay: `drain`'s `Drop` clears the whole range however far iteration got, so an early
        // return discards the untried candidates instead of leaving them queued. It would also
        // skip the `restarting_ice` reset below, leaving the transport queuing every future
        // candidate forever. The remote description is already applied at this point, so there
        // is nothing to unwind — the failure is per-candidate and the rest still stand.
        for ic in inner.pending_candidates.drain(..) {
            let candidate = ic.to_string();
            if let Err(err) = self.peer_connection.add_ice_candidate(ic).await {
                log::warn!(
                    "{:?}: failed to add pending ice candidate {}: {:?}",
                    self.signal_target,
                    candidate,
                    err
                );
            }
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
        let offer_sdp = offer.to_string();
        self.set_remote_description(offer).await?;
        let answer = self.peer_connection().create_answer(options).await?;
        let answer = Self::munge_opus_nack_for_answer(&answer.to_string(), &offer_sdp)
            .and_then(|munged| Self::parse_munged(&munged, answer.sdp_type()))
            .unwrap_or(answer);
        self.peer_connection().set_local_description(answer.clone()).await?;

        Ok(answer)
    }

    /// Create an initial offer without setting it as local description.
    /// The offer is stored as pending and will be applied when the server's answer arrives.
    ///
    /// It gets the same munging as [`Self::create_and_send_offer`], including the
    /// `x-google-start-bitrate` hint when join-time video tracks are present. That hint is
    /// computed without a connection setup time, since the connection does not exist yet, so it
    /// stays capped at [`Self::MAX_START_BITRATE_KBPS`].
    pub async fn create_initial_offer(&self) -> EngineResult<Option<SessionDescription>> {
        let inner = self.inner.lock().await;
        if !inner.single_pc_mode {
            return Ok(None);
        }
        drop(inner);

        let offer = self
            .peer_connection
            .create_offer(Self::single_pc_offer_options(OfferOptions::default()))
            .await?;

        let mut inner = self.inner.lock().await;
        let munged = Self::munge_offer(&inner, offer);
        let offer = munged.offer.clone();
        inner.pending_initial_offer = Some(munged);
        Ok(Some(offer))
    }

    pub async fn set_max_send_bitrate_bps(&self, bps: Option<u64>, is_screen_share: bool) {
        let mut inner = self.inner.lock().await;
        inner.max_send_bitrate_bps = bps;
        inner.max_send_bitrate_is_screen_share = is_screen_share;
    }

    /// Maximum x-google-start-bitrate (kbps).
    /// 1 Mbps is a reasonable ceiling that prevents BWE from starting too aggressively.
    const MAX_START_BITRATE_KBPS: u32 = 1000;

    /// Minimum x-google-start-bitrate (kbps): libwebrtc's own default starting estimate. A
    /// target below this gets no hint, since seeding above the real capacity costs more than
    /// the ramp it saves, and a slow connection is seeded no higher than this.
    const MIN_TARGET_BITRATE_KBPS: u32 = 300;

    /// Connection setup times bounding the ramp in [`Self::compute_start_bitrate_kbps`]: at
    /// or below the first the hint is capped at [`Self::MAX_START_BITRATE_KBPS`], at or above
    /// the second at [`Self::MIN_TARGET_BITRATE_KBPS`]. Measured on shaped links (CLT-3380):
    /// healthy links set up in under 860 ms, a 1 Mbps link with 150 ms RTT took 1.1–1.7 s,
    /// a 500 kbps link ~2.3 s at the median, and a 300 kbps link never under 3 s. The first
    /// sits above the 1 Mbps link, which a lower seed only slows down, and the second above
    /// the 500 kbps link's median, so only links that cannot carry more land at the floor.
    const SETUP_TIME_FOR_MAX_BITRATE: Duration = Duration::from_millis(1500);
    const SETUP_TIME_FOR_MIN_BITRATE: Duration = Duration::from_millis(3500);

    /// Record how long this peer connection took to set up. Called once, after the initial
    /// connect succeeds; resumes and ICE restarts keep the estimator and never call this.
    pub async fn set_connection_setup_time(&self, setup: Duration) {
        self.inner.lock().await.connection_setup_time = Some(setup);
    }

    /// Compute the x-google-start-bitrate value for SDP munging.
    ///
    /// 90% of the target leaves ~10% headroom for the estimator to settle. The same
    /// multiplier is used for every codec because the target already reflects the codec's
    /// efficiency. Camera is capped at [`Self::MAX_START_BITRATE_KBPS`] so the estimator does
    /// not open too aggressively on a high-bitrate track; screen share is exempt, because its
    /// content needs the bitrate immediately to stay legible.
    ///
    /// Connection setup time (signaling join plus ICE/DTLS) is the only network signal there
    /// is before the first video offer, since libwebrtc cannot probe the path until a video
    /// sender exists. It grows with round-trip time and loss, which also mark the links where
    /// a 1 Mbps seed overshoots, so the cap ramps linearly from
    /// [`Self::MAX_START_BITRATE_KBPS`] at [`Self::SETUP_TIME_FOR_MAX_BITRATE`] down to
    /// [`Self::MIN_TARGET_BITRATE_KBPS`] at [`Self::SETUP_TIME_FOR_MIN_BITRATE`]. Screen
    /// share is exempt from this ramp as well, however slow the setup. Without a setup time
    /// (an offer before the initial connect completed) the cap stays at the 1 Mbps ceiling.
    ///
    /// Returns None if no target bitrate is set (initial offer before track publish) or if
    /// the target is below [`Self::MIN_TARGET_BITRATE_KBPS`].
    fn compute_start_bitrate_kbps(
        target_bps: Option<u64>,
        is_screen_share: bool,
        setup_time: Option<Duration>,
    ) -> Option<u32> {
        let target_bps = target_bps?;
        let target_kbps = (target_bps / 1000) as u32;

        if target_kbps < Self::MIN_TARGET_BITRATE_KBPS {
            return None;
        }

        let cap_kbps = match setup_time {
            None => Self::MAX_START_BITRATE_KBPS,
            Some(setup) => {
                let fast = Self::SETUP_TIME_FOR_MAX_BITRATE.as_secs_f64();
                let slow = Self::SETUP_TIME_FOR_MIN_BITRATE.as_secs_f64();
                let ramp = ((setup.as_secs_f64() - fast) / (slow - fast)).clamp(0.0, 1.0);
                let range = (Self::MAX_START_BITRATE_KBPS - Self::MIN_TARGET_BITRATE_KBPS) as f64;
                (Self::MAX_START_BITRATE_KBPS as f64 - ramp * range).round() as u32
            }
        };

        let start_kbps = ((target_kbps as f64 * 0.9).round() as u32).min(target_kbps);
        if is_screen_share {
            Some(start_kbps)
        } else {
            Some(start_kbps.min(cap_kbps))
        }
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

    /// Parses munged SDP, logging and returning `None` if libwebrtc rejects it.
    fn parse_munged(munged: &str, sdp_type: SdpType) -> Option<SessionDescription> {
        SessionDescription::parse(munged, sdp_type)
            .inspect_err(|e| log::warn!("Failed to parse munged SDP, using original: {e}"))
            .ok()
    }

    /// Adds Opus NACK to each audio section where `wants_nack` holds, since libwebrtc offers
    /// only `transport-cc`. Returns `None` if there is nothing to add.
    fn munge_opus_nack(sdp: &str, wants_nack: impl Fn(SdpSection) -> bool) -> Option<String> {
        let eol = if sdp.contains("\r\n") { "\r\n" } else { "\n" };
        let lines: Vec<&str> = sdp.split_inclusive('\n').collect();
        let mut munged = String::with_capacity(sdp.len());
        let mut changed = false;

        for section in SdpSection::split(&lines) {
            let add_nack = section.is_audio() && wants_nack(section);
            for line in section.lines {
                munged.push_str(line);
                let Some(pt) = opus_pt(line) else { continue };
                if !add_nack || section.has_nack(pt) {
                    continue;
                }
                if !line.ends_with('\n') {
                    munged.push_str(eol);
                }
                munged.extend(["a=rtcp-fb:", pt, " nack", eol]);
                changed = true;
            }
        }

        changed.then_some(munged)
    }

    /// Adds Opus NACK to every audio section of a single PC offer, whatever its direction.
    ///
    /// The server takes audio feedback from the first audio section of the first offer only.
    /// That section can be the client's own `sendonly` mic, so every section needs NACK.
    fn munge_opus_nack_for_offer(sdp: &str) -> Option<String> {
        Self::munge_opus_nack(sdp, |_| true)
    }

    /// Adds Opus NACK to each audio section of a dual PC answer whose counterpart in `offer`
    /// (matched by mid) has it. libwebrtc drops it from the answer otherwise.
    fn munge_opus_nack_for_answer(answer: &str, offer: &str) -> Option<String> {
        let offer_lines: Vec<&str> = offer.split_inclusive('\n').collect();
        let nack_mids: Vec<&str> = SdpSection::split(&offer_lines)
            .filter(|section| {
                section.is_audio() && section.opus_pts().any(|pt| section.has_nack(pt))
            })
            .filter_map(SdpSection::mid)
            .collect();

        Self::munge_opus_nack(answer, |section| {
            section.mid().is_some_and(|mid| nack_mids.contains(&mid))
        })
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

    /// Leaves libwebrtc's legacy `offer_to_receive_*` options unset. The publisher receives
    /// on its own receive-only transceivers in single PC mode, and an explicit `false` would
    /// make libwebrtc turn them inactive.
    fn single_pc_offer_options(options: OfferOptions) -> OfferOptions {
        OfferOptions { offer_to_receive_audio: None, offer_to_receive_video: None, ..options }
    }

    /// Applies the local SDP munging to a freshly created offer.
    fn munge_offer(inner: &TransportInner, mut offer: SessionDescription) -> MungedOffer {
        let mut sdp = offer.to_string();

        if inner.single_pc_mode {
            // Apply stereo and Opus NACK munging for single PC mode
            let stereo = Self::munge_stereo_for_audio(&sdp);
            let munged = Self::munge_opus_nack_for_offer(&stereo).unwrap_or(stereo);
            if munged != sdp {
                if let Some(parsed) = Self::parse_munged(&munged, offer.sdp_type()) {
                    offer = parsed;
                    sdp = munged;
                }
            }
        }

        // Apply x-google-start-bitrate for all video codecs to improve initial quality.
        //
        // The same value goes on every video codec, and only on the first offer that carries
        // local video. libwebrtc reads this fmtp parameter per m-section but applies it to the
        // shared Call (`WebRtcVideoSendChannel::ApplyChangedParams` -> `SetSdpBitrateParameters`),
        // where `RtpBitrateConfigurator` keeps one config for the whole peer connection: it
        // retains `start_bitrate_bps` and re-applies it on network route changes, so rewriting
        // it later is at best a no-op and at worst restarts a converged bandwidth estimator. A
        // full reconnect builds a new peer connection and seeds the new estimator again.
        //
        // x-google-max-bitrate is deliberately never written: the same Call-level promotion
        // would turn a per-track cap into a ceiling on total send bandwidth, starving concurrent
        // tracks. Per-track and per-layer caps belong in the encodings' max_bitrate.
        let mut has_start_bitrate = false;
        let has_video = sdp.contains(" VP8/90000")
            || sdp.contains(" VP9/90000")
            || sdp.contains(" AV1/90000")
            || sdp.contains(" H264/90000")
            || sdp.contains(" H265/90000");
        if has_video && !inner.start_bitrate_applied {
            if let Some(start_kbps) = Self::compute_start_bitrate_kbps(
                inner.max_send_bitrate_bps,
                inner.max_send_bitrate_is_screen_share,
                inner.connection_setup_time,
            ) {
                log::info!(
                    "Applying x-google-start-bitrate={} kbps (target_bps={:?}, connection_setup_ms={:?})",
                    start_kbps,
                    inner.max_send_bitrate_bps,
                    inner.connection_setup_time.map(|t| t.as_millis())
                );

                let munged = Self::munge_x_google_start_bitrate(&sdp, start_kbps);
                if munged != sdp {
                    log::debug!("SDP munged successfully for video codec");
                    match SessionDescription::parse(&munged, offer.sdp_type()) {
                        Ok(parsed) => {
                            offer = parsed;
                            has_start_bitrate = true;
                        }
                        Err(e) => log::warn!(
                            "Failed to parse munged SDP, falling back to original offer: {e}"
                        ),
                    }
                } else {
                    log::debug!("SDP munging produced no changes");
                }
            }
        }

        MungedOffer { offer, has_start_bitrate }
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

        let options =
            if inner.single_pc_mode { Self::single_pc_offer_options(options) } else { options };
        let offer = self.peer_connection.create_offer(options).await?;
        let MungedOffer { offer, has_start_bitrate } = Self::munge_offer(&inner, offer);

        self.peer_connection.set_local_description(offer.clone()).await?;

        // Only consume the one-shot hint once the offer carrying it is accepted locally.
        if has_start_bitrate {
            inner.start_bitrate_applied = true;
        }

        if let Some(handler) = self.on_offer_handler.lock().as_mut() {
            handler(offer);
        }

        Ok(())
    }
}

/// A section of an SDP, the session part or one m-section, as lines with their line endings.
/// `'s` borrows the lines and `'a` the SDP text.
#[derive(Clone, Copy)]
struct SdpSection<'s, 'a> {
    lines: &'s [&'a str],
}

impl<'s, 'a> SdpSection<'s, 'a> {
    /// Splits SDP lines into the session section followed by one section per `m=` line.
    fn split(lines: &'s [&'a str]) -> impl Iterator<Item = Self> {
        lines.chunk_by(|_, next| !next.starts_with("m=")).map(|lines| Self { lines })
    }

    /// The values of the `a=` lines, without the `a=` prefix and the line ending.
    fn attributes(self) -> impl Iterator<Item = &'a str> + use<'s, 'a> {
        self.lines.iter().filter_map(|line| line.trim_end().strip_prefix("a="))
    }

    /// Whether this is an audio m-section.
    fn is_audio(self) -> bool {
        self.lines.first().is_some_and(|line| line.starts_with("m=audio "))
    }

    /// The value of the `a=mid` line.
    fn mid(self) -> Option<&'a str> {
        self.attributes().find_map(|attribute| attribute.strip_prefix("mid:"))
    }

    /// The Opus payload types, from the `a=rtpmap` lines.
    fn opus_pts(self) -> impl Iterator<Item = &'a str> + use<'s, 'a> {
        self.lines.iter().copied().filter_map(opus_pt)
    }

    /// Whether payload type `pt` has plain `nack` feedback, directly or through the `*`
    /// wildcard. `nack pli` is a different feedback type and does not count.
    fn has_nack(self, pt: &str) -> bool {
        self.attributes().filter_map(|attribute| attribute.strip_prefix("rtcp-fb:")).any(
            |feedback| {
                let mut fields = feedback.split_whitespace();
                matches!(fields.next(), Some(fb_pt) if fb_pt == pt || fb_pt == "*")
                    && fields.next() == Some("nack")
                    && fields.next().is_none()
            },
        )
    }
}

/// Returns the payload type of an `a=rtpmap` line for Opus, matching the name in any case.
fn opus_pt(line: &str) -> Option<&str> {
    let mut fields = line.trim_end().strip_prefix("a=rtpmap:")?.split_whitespace();
    let pt = fields.next()?;
    fields.next()?.split('/').next()?.eq_ignore_ascii_case("opus").then_some(pt)
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use libwebrtc::prelude::{PeerConnection, PeerConnectionFactory, RtpTransceiverDirection};
    use parking_lot::Mutex;

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
        let mut config = RtcConfiguration::default();
        config.continual_gathering_policy = ContinualGatheringPolicy::GatherOnce;
        config.enable_sctp_snap = true;

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

    /// Join-time video makes the initial offer carry the start bitrate hint, which is consumed
    /// only once the server's answer applies the offer as the local description.
    #[tokio::test]
    async fn initial_offer_carries_start_bitrate_hint() {
        use libwebrtc::prelude::*;
        use livekit_protocol as proto;

        let factory = PeerConnectionFactory::default();
        let alice_pc = factory.create_peer_connection(RtcConfiguration::default()).unwrap();
        let bob_pc = factory.create_peer_connection(RtcConfiguration::default()).unwrap();

        let init = RtpTransceiverInit {
            direction: RtpTransceiverDirection::SendOnly,
            stream_ids: Vec::new(),
            send_encodings: Vec::new(),
        };
        alice_pc.add_transceiver_for_media(MediaType::Video, init).unwrap();

        let transport = PeerTransport::new(
            alice_pc,
            proto::SignalTarget::Publisher,
            /* single_pc_mode= */ true,
        );
        transport.set_max_send_bitrate_bps(Some(2_000_000), false).await;

        let offer = transport.create_initial_offer().await.unwrap().expect("single PC mode");
        // 90% of the 2 Mbps target, capped at 1 Mbps without a connection setup time.
        assert!(offer.to_string().contains("x-google-start-bitrate=1000"));
        assert!(!transport.inner.lock().await.start_bitrate_applied);

        bob_pc.set_remote_description(offer).await.unwrap();
        let answer = bob_pc.create_answer(AnswerOptions::default()).await.unwrap();
        bob_pc.set_local_description(answer.clone()).await.unwrap();

        transport.set_remote_description(answer).await.unwrap();
        assert!(transport.inner.lock().await.start_bitrate_applied);
    }

    /// The two counters are the whole basis on which a resume decides a transport recovered,
    /// so each transition must land on exactly one of them, and a transport that drops and
    /// returns must leave both marks rather than looking untouched.
    #[test]
    fn connection_state_transitions_are_counted_separately() {
        use libwebrtc::prelude::*;
        use livekit_protocol as proto;

        let factory = PeerConnectionFactory::default();
        let mut config = RtcConfiguration::default();
        config.continual_gathering_policy = ContinualGatheringPolicy::GatherOnce;
        config.ice_transport_type = IceTransportsType::All;
        let pc = factory.create_peer_connection(config).unwrap();

        let transport = PeerTransport::new(
            pc,
            proto::SignalTarget::Subscriber,
            /* single_pc_mode= */ false,
        );
        let counters = || (transport.connected_generation(), transport.disconnect_generation());

        assert_eq!(counters(), (0, 0));

        transport.note_connection_state(PeerConnectionState::Connecting);
        assert_eq!(counters(), (0, 1));

        transport.note_connection_state(PeerConnectionState::Connected);
        assert_eq!(counters(), (1, 1));

        // Dropping and returning must move both counters: a resume needs to see that it broke
        // *and* that it came back, and a poll sampling only the current state sees neither.
        transport.note_connection_state(PeerConnectionState::Disconnected);
        transport.note_connection_state(PeerConnectionState::Connected);
        assert_eq!(counters(), (2, 2));

        transport.note_connection_state(PeerConnectionState::Failed);
        assert_eq!(counters(), (2, 3));
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
    fn start_bitrate_is_ninety_percent_of_target() {
        assert_eq!(
            PeerTransport::compute_start_bitrate_kbps(Some(1_000_000), false, None),
            Some(900),
            "should be 90% of a 1 Mbps target"
        );
    }

    #[test]
    fn start_bitrate_caps_camera_but_not_screen_share() {
        // 90% of 3 Mbps is 2700, above the 1 Mbps camera ceiling.
        assert_eq!(
            PeerTransport::compute_start_bitrate_kbps(Some(3_000_000), false, None),
            Some(PeerTransport::MAX_START_BITRATE_KBPS),
            "camera should be capped"
        );
        assert_eq!(
            PeerTransport::compute_start_bitrate_kbps(Some(3_000_000), true, None),
            Some(2700),
            "screen share should not be capped; its content needs the bitrate to stay legible"
        );
    }

    #[test]
    fn start_bitrate_skipped_below_target_floor() {
        assert_eq!(
            PeerTransport::compute_start_bitrate_kbps(Some(299_000), false, None),
            None,
            "below the floor libwebrtc's own default is left in place"
        );
        assert_eq!(
            PeerTransport::compute_start_bitrate_kbps(Some(300_000), false, None),
            Some(270),
            "at the floor the hint applies"
        );
        assert_eq!(
            PeerTransport::compute_start_bitrate_kbps(None, false, None),
            None,
            "no target means no hint (initial offer before any publish)"
        );
    }

    #[test]
    fn start_bitrate_ramps_down_with_connection_setup_time() {
        // A 3 Mbps camera target, so only the cap moves.
        let camera = |ms| {
            PeerTransport::compute_start_bitrate_kbps(
                Some(3_000_000),
                false,
                Some(Duration::from_millis(ms)),
            )
        };
        assert_eq!(camera(0), Some(1000), "instant setup keeps the ceiling");
        assert_eq!(camera(471), Some(1000), "unshaped baseline keeps the ceiling");
        assert_eq!(camera(1273), Some(1000), "1 Mbps link median keeps the ceiling");
        assert_eq!(camera(1500), Some(1000), "the fast anchor keeps the ceiling");
        assert_eq!(camera(1724), Some(922), "1 Mbps link slow attempt barely moves");
        assert_eq!(camera(2334), Some(708), "500 kbps link median lands mid-ramp");
        assert_eq!(camera(2500), Some(650), "midpoint of the ramp");
        assert_eq!(camera(3093), Some(442), "300 kbps link fastest attempt");
        assert_eq!(camera(3500), Some(300), "the slow anchor reaches the floor");
        assert_eq!(camera(18_131), Some(300), "anything slower stays at the floor");

        assert_eq!(
            PeerTransport::compute_start_bitrate_kbps(
                Some(500_000),
                false,
                Some(Duration::from_millis(2500))
            ),
            Some(450),
            "90% of the target still wins when it is lower than the cap"
        );
        assert_eq!(
            PeerTransport::compute_start_bitrate_kbps(
                Some(300_000),
                false,
                Some(Duration::from_millis(3500))
            ),
            Some(270),
            "the hint is still written at the floor rather than skipped"
        );
    }

    #[test]
    fn slow_connection_setup_leaves_screen_share_uncapped() {
        let screen_share = |ms| {
            PeerTransport::compute_start_bitrate_kbps(
                Some(3_000_000),
                true,
                Some(Duration::from_millis(ms)),
            )
        };
        assert_eq!(screen_share(1273), Some(2700), "a fast setup");
        assert_eq!(screen_share(2500), Some(2700), "mid-ramp");
        assert_eq!(screen_share(4061), Some(2700), "past the slow anchor");
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

    /// An audio m-section with Opus on payload type 111, as libwebrtc writes it.
    fn audio_section(mid: &str, direction: &str, extra: &str) -> String {
        format!(
            "m=audio 9 UDP/TLS/RTP/SAVPF 111 0\r\n\
a=mid:{mid}\r\n\
a={direction}\r\n\
a=rtpmap:111 OPUS/48000/2\r\n\
{extra}\
a=rtcp-fb:111 transport-cc\r\n\
a=fmtp:111 minptime=10;useinbandfec=1\r\n\
a=rtpmap:0 PCMU/8000\r\n"
        )
    }

    fn sdp(sections: &[String]) -> String {
        format!("v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n{}", sections.concat())
    }

    const NACK: &str = "a=rtcp-fb:111 nack\r\n";

    #[test]
    fn offer_nack_on_every_audio_section() {
        let video = "m=video 9 UDP/TLS/RTP/SAVPF 96\r\na=mid:3\r\na=rtpmap:96 VP8/90000\r\n";
        let offer = sdp(&[
            audio_section("0", "sendonly", ""),
            audio_section("1", "recvonly", ""),
            audio_section("2", "sendrecv", ""),
            video.to_string(),
        ]);
        let out = PeerTransport::munge_opus_nack_for_offer(&offer);
        let expected = sdp(&[
            audio_section("0", "sendonly", NACK),
            audio_section("1", "recvonly", NACK),
            audio_section("2", "sendrecv", NACK),
            video.to_string(),
        ]);
        assert_eq!(out, Some(expected));
    }

    /// Every Opus payload type gets NACK, with LF or CRLF line endings. `nack pli` is not plain
    /// NACK, and other codecs and sections, even with the same payload type, stay unchanged.
    #[test]
    fn offer_nack_on_every_opus_payload_type() {
        for eol in ["\n", "\r\n"] {
            let offer = [
                "v=0",
                "m=audio 9 UDP/TLS/RTP/SAVPF 109 112 0",
                "a=rtpmap:109 OPUS/48000/2",
                "a=rtcp-fb:109 nack pli",
                "a=rtpmap:112 opus/48000/2",
                "a=rtpmap:0 PCMU/8000",
                "m=video 9 UDP/TLS/RTP/SAVPF 109",
                "a=rtpmap:109 VP8/90000",
                "",
            ]
            .join(eol);
            let expected = [
                "v=0",
                "m=audio 9 UDP/TLS/RTP/SAVPF 109 112 0",
                "a=rtpmap:109 OPUS/48000/2",
                "a=rtcp-fb:109 nack",
                "a=rtcp-fb:109 nack pli",
                "a=rtpmap:112 opus/48000/2",
                "a=rtcp-fb:112 nack",
                "a=rtpmap:0 PCMU/8000",
                "m=video 9 UDP/TLS/RTP/SAVPF 109",
                "a=rtpmap:109 VP8/90000",
                "",
            ]
            .join(eol);
            let out = PeerTransport::munge_opus_nack_for_offer(&offer);
            assert_eq!(out.as_deref(), Some(expected.as_str()));
            assert_eq!(PeerTransport::munge_opus_nack_for_offer(&expected), None);
        }
    }

    /// An Opus `a=rtpmap` as the last line, without a line ending, still gets NACK on its own line.
    #[test]
    fn offer_nack_after_last_line_without_line_ending() {
        for eol in ["\n", "\r\n"] {
            let offer =
                ["v=0", "m=audio 9 UDP/TLS/RTP/SAVPF 111", "a=rtpmap:111 opus/48000/2"].join(eol);
            let expected = format!("{offer}{eol}a=rtcp-fb:111 nack{eol}");
            assert_eq!(PeerTransport::munge_opus_nack_for_offer(&offer), Some(expected));
        }
    }

    /// Existing plain NACK, also through the `*` wildcard or with a tab, is not added twice,
    /// and it counts as NACK in an offer that an answer mirrors.
    #[test]
    fn existing_nack_is_kept_and_mirrored() {
        for feedback in
            ["a=rtcp-fb:111 nack\r\n", "a=rtcp-fb:* nack\r\n", "a=rtcp-fb:111\tnack\r\n"]
        {
            let offer = sdp(&[audio_section("0", "sendonly", feedback)]);
            assert_eq!(PeerTransport::munge_opus_nack_for_offer(&offer), None);

            let answer = sdp(&[audio_section("0", "recvonly", "")]);
            assert_eq!(
                PeerTransport::munge_opus_nack_for_answer(&answer, &offer),
                Some(sdp(&[audio_section("0", "recvonly", NACK)]))
            );
        }
        let no_opus = "v=0\nm=audio 9 UDP/TLS/RTP/SAVPF 0\na=rtpmap:0 PCMU/8000\n";
        assert_eq!(PeerTransport::munge_opus_nack_for_offer(no_opus), None);
        assert_eq!(PeerTransport::munge_opus_nack_for_offer(""), None);
    }

    #[test]
    fn answer_nack_mirrors_offer_by_mid() {
        let offer =
            sdp(&[audio_section("0", "sendonly", NACK), audio_section("1", "sendonly", "")]);
        let answer = sdp(&[audio_section("0", "recvonly", ""), audio_section("1", "recvonly", "")]);
        let out = PeerTransport::munge_opus_nack_for_answer(&answer, &offer);
        let expected =
            sdp(&[audio_section("0", "recvonly", NACK), audio_section("1", "recvonly", "")]);
        assert_eq!(out, Some(expected));
    }

    #[test]
    fn answer_untouched_when_offer_has_no_nack() {
        let offer = sdp(&[audio_section("0", "sendonly", "")]);
        let answer = sdp(&[audio_section("0", "recvonly", "")]);
        assert_eq!(PeerTransport::munge_opus_nack_for_answer(&answer, &offer), None);
    }

    // The tests below run the munges on SDP that libwebrtc writes and parses. They are adapted
    // from livekit/rust-sdks#1695.

    const ALL_DIRECTIONS: [RtpTransceiverDirection; 3] = [
        RtpTransceiverDirection::SendOnly,
        RtpTransceiverDirection::RecvOnly,
        RtpTransceiverDirection::SendRecv,
    ];

    /// Creates a peer connection with one audio transceiver per direction.
    fn pc_with_audio(
        factory: &PeerConnectionFactory,
        directions: &[RtpTransceiverDirection],
    ) -> PeerConnection {
        use libwebrtc::prelude::{MediaType, RtcConfiguration, RtpTransceiverInit};

        let pc = factory.create_peer_connection(RtcConfiguration::default()).unwrap();
        for &direction in directions {
            pc.add_transceiver_for_media(
                MediaType::Audio,
                RtpTransceiverInit { direction, stream_ids: vec![], send_encodings: vec![] },
            )
            .unwrap();
        }
        pc
    }

    /// For each audio section of `sdp`, whether every Opus payload type has plain NACK.
    fn audio_nack(sdp: &str) -> Vec<bool> {
        sdp.split("\nm=")
            .filter(|section| section.starts_with("audio "))
            .map(|section| {
                let pts: Vec<&str> = section
                    .lines()
                    .filter_map(|line| {
                        let (pt, codec) = line.strip_prefix("a=rtpmap:")?.split_once(' ')?;
                        codec.to_ascii_lowercase().starts_with("opus/").then_some(pt)
                    })
                    .collect();
                assert!(!pts.is_empty(), "audio section without Opus: {section}");
                pts.iter()
                    .all(|pt| section.lines().any(|line| line == format!("a=rtcp-fb:{pt} nack")))
            })
            .collect()
    }

    /// The initial offer has NACK in every direction, the stored offer is the one sent with
    /// the join, and NACK is still there once that offer is set locally.
    #[tokio::test]
    async fn single_pc_initial_offer_has_opus_nack() {
        use libwebrtc::prelude::*;
        use livekit_protocol as proto;

        let factory = PeerConnectionFactory::default();
        let transport = PeerTransport::new(
            pc_with_audio(&factory, &ALL_DIRECTIONS),
            proto::SignalTarget::Publisher,
            /* single_pc_mode= */ true,
        );
        let offer = transport.create_initial_offer().await.unwrap().expect("no initial offer");
        assert_eq!(audio_nack(&offer.to_string()), [true; 3]);
        let pending = transport
            .inner
            .lock()
            .await
            .pending_initial_offer
            .as_ref()
            .map(|p| p.offer.to_string());
        assert_eq!(pending, Some(offer.to_string()), "stored offer differs from the sent offer");

        let remote = factory.create_peer_connection(RtcConfiguration::default()).unwrap();
        remote.set_remote_description(offer).await.unwrap();
        let answer = remote.create_answer(AnswerOptions::default()).await.unwrap();
        remote.set_local_description(answer.clone()).await.unwrap();
        transport.set_remote_description(answer).await.unwrap();
        let local = transport.peer_connection().current_local_description().unwrap();
        assert_eq!(audio_nack(&local.to_string()), [true; 3]);

        transport.close();
        remote.close();
    }

    /// Later offers have NACK in every direction in single PC mode, and none in dual PC mode.
    #[tokio::test]
    async fn offers_have_opus_nack_only_in_single_pc_mode() {
        use libwebrtc::prelude::*;
        use livekit_protocol as proto;

        let factory = PeerConnectionFactory::default();
        for single_pc in [true, false] {
            let transport = PeerTransport::new(
                pc_with_audio(&factory, &ALL_DIRECTIONS),
                proto::SignalTarget::Publisher,
                single_pc,
            );
            let offers = Arc::new(Mutex::new(Vec::new()));
            let captured = Arc::clone(&offers);
            transport.on_offer(Some(Box::new(move |offer| captured.lock().push(offer))));

            transport.create_and_send_offer(OfferOptions::default()).await.unwrap();
            let offer = offers.lock().last().expect("no offer sent").to_string();
            assert_eq!(audio_nack(&offer), [single_pc; 3], "single_pc_mode={single_pc}");
            transport.close();
        }
    }

    /// A dual PC answer has NACK exactly when the remote offer has it. libwebrtc alone drops
    /// it from the answer.
    #[tokio::test]
    async fn dual_pc_answer_mirrors_offer_nack() {
        use libwebrtc::prelude::*;
        use livekit_protocol as proto;

        let factory = PeerConnectionFactory::default();
        for offer_nack in [true, false] {
            let remote = pc_with_audio(&factory, &[RtpTransceiverDirection::SendOnly]);
            let mut offer = remote.create_offer(OfferOptions::default()).await.unwrap();
            if offer_nack {
                let sdp = PeerTransport::munge_opus_nack_for_offer(&offer.to_string())
                    .expect("libwebrtc offered Opus NACK already");
                offer = SessionDescription::parse(&sdp, offer.sdp_type()).unwrap();
            }
            assert_eq!(audio_nack(&offer.to_string()), [offer_nack]);
            remote.set_local_description(offer.clone()).await.unwrap();

            let transport = PeerTransport::new(
                factory.create_peer_connection(RtcConfiguration::default()).unwrap(),
                proto::SignalTarget::Subscriber,
                /* single_pc_mode= */ false,
            );
            let answer = transport.create_anwser(offer, AnswerOptions::default()).await.unwrap();
            assert_eq!(audio_nack(&answer.to_string()), [offer_nack], "offer_nack={offer_nack}");
            let local = transport.peer_connection().current_local_description().unwrap();
            assert_eq!(audio_nack(&local.to_string()), [offer_nack]);
            remote.set_remote_description(answer).await.unwrap();

            transport.close();
            remote.close();
        }
    }
}
