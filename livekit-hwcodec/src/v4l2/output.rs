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

//! Checks that the encoder's output is usable: successful ioctls don't prove it.

use std::{
    collections::VecDeque,
    fmt,
    time::{Duration, Instant},
};

use livekit_bitstream::{annex_b_nalus, h264_has_parameter_sets, h264_is_keyframe, h264_nal_type};
use thiserror::Error;

use super::{session::Encoded, sys::FourCc};
use crate::FrameMeta;

// Provisional, to be calibrated on hardware.
pub(super) const WATCHDOG: Duration = Duration::from_millis(500);
const STARTUP_KEYFRAME_FRAMES: u32 = 60;
const KEYFRAME_FRAMES: u32 = 10;
const MAX_BAD_OUTPUTS: u32 = 5;

/// Drops one frame.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub(super) enum BadOutput {
    #[error("flagged as corrupt by the driver")]
    Corrupt,
    #[error("empty")]
    Empty,
    #[error("not an Annex-B bitstream")]
    NotAnnexB,
    #[error("matches no queued frame")]
    Unmatched,
}

/// Ends the session.
#[derive(Debug, Error, PartialEq, Eq)]
pub(super) enum OutputError {
    #[error("no output for {0:?} while the encoder holds frames")]
    Stalled(Duration),
    #[error("no keyframe within {0} frames of being due")]
    NoKeyframe(u32),
    #[error("keyframe without SPS and PPS")]
    KeyframeWithoutParameterSets,
    #[error("{count} unusable outputs in a row, the last one {last}")]
    BadOutputs { count: u32, last: BadOutput },
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Verdict {
    pub dropped: Vec<FrameMeta>,
    pub deliver: Option<Delivery>,
    pub bad: Option<BadOutput>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Delivery {
    pub meta: FrameMeta,
    pub keyframe: bool,
    /// Parameter sets that came in a buffer of their own.
    pub headers: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Stats {
    pub queued: u64,
    pub delivered: u64,
    pub dropped: u64,
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "frames in {}, out {}, dropped {}", self.queued, self.delivered, self.dropped)
    }
}

struct Pending {
    timestamp: u64,
    meta: FrameMeta,
}

/// A keyframe that is due.
struct Awaiting {
    frames: u32,
    limit: u32,
    /// Nothing before the session's first keyframe can be decoded.
    first: bool,
}

pub(super) struct Tracker {
    coded: FourCc,
    pending: VecDeque<Pending>,
    headers: Option<Vec<u8>>,
    awaiting: Option<Awaiting>,
    bad_outputs: u32,
    last_progress: Instant,
    stats: Stats,
}

impl Tracker {
    pub(super) fn new(coded: FourCc, now: Instant) -> Self {
        let mut tracker = Self {
            coded,
            pending: VecDeque::new(),
            headers: None,
            awaiting: None,
            bad_outputs: 0,
            last_progress: now,
            stats: Stats::default(),
        };
        tracker.expect_keyframe(STARTUP_KEYFRAME_FRAMES, true);
        tracker
    }

    pub(super) fn stats(&self) -> Stats {
        self.stats
    }

    pub(super) fn queued(&mut self, timestamp: u64, meta: FrameMeta, now: Instant) {
        if self.pending.is_empty() {
            self.last_progress = now;
        }
        self.pending.push_back(Pending { timestamp, meta });
        self.stats.queued += 1;
    }

    pub(super) fn keyframe_requested(&mut self) {
        self.expect_keyframe(KEYFRAME_FRAMES, false);
    }

    fn expect_keyframe(&mut self, limit: u32, first: bool) {
        // Only bitstreams this crate parses can be checked.
        if self.coded == FourCc::H264 && self.awaiting.is_none() {
            self.awaiting = Some(Awaiting { frames: 0, limit, first });
        }
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        (!self.pending.is_empty()).then(|| self.last_progress + WATCHDOG)
    }

    pub(super) fn check(&self, now: Instant) -> Result<(), OutputError> {
        let idle = now.saturating_duration_since(self.last_progress);
        if !self.pending.is_empty() && idle > WATCHDOG {
            return Err(OutputError::Stalled(idle));
        }
        Ok(())
    }

    pub(super) fn output(
        &mut self,
        output: &Encoded<'_>,
        now: Instant,
    ) -> Result<Verdict, OutputError> {
        self.last_progress = now;
        let mut verdict = Verdict::default();

        let contents = (self.coded == FourCc::H264).then(|| H264Contents::of(output.data));
        if let Some(Some(contents)) = &contents {
            if contents.parameter_sets && !contents.picture && !output.corrupt {
                // Separate header mode: SPS and PPS go in front of the next frame.
                self.headers = Some(output.data.to_vec());
                return Ok(verdict);
            }
        }

        let position = self.pending.iter().position(|frame| frame.timestamp == output.timestamp);
        let Some(position) = position else {
            return self.bad(BadOutput::Unmatched, verdict);
        };
        // The encoder skipped the frames before this one.
        verdict.dropped.extend(self.pending.drain(..position).map(|frame| frame.meta));
        let Some(frame) = self.pending.pop_front() else {
            return Ok(verdict);
        };

        let bad = if output.corrupt {
            Some(BadOutput::Corrupt)
        } else if output.data.is_empty() {
            Some(BadOutput::Empty)
        } else if matches!(contents, Some(None)) {
            Some(BadOutput::NotAnnexB)
        } else {
            None
        };
        if let Some(bad) = bad {
            verdict.dropped.push(frame.meta);
            return self.bad(bad, verdict);
        }
        self.bad_outputs = 0;

        let keyframe = match contents.flatten() {
            Some(contents) => {
                if contents.idr && !contents.parameter_sets && self.headers.is_none() {
                    return Err(OutputError::KeyframeWithoutParameterSets);
                }
                contents.idr
            }
            None => output.keyframe,
        };
        if keyframe {
            self.awaiting = None;
        } else if let Some(awaiting) = &mut self.awaiting {
            awaiting.frames += 1;
            if awaiting.frames > awaiting.limit {
                return Err(OutputError::NoKeyframe(awaiting.limit));
            }
            if awaiting.first {
                verdict.dropped.push(frame.meta);
                self.stats.dropped += verdict.dropped.len() as u64;
                return Ok(verdict);
            }
        }

        self.stats.dropped += verdict.dropped.len() as u64;
        self.stats.delivered += 1;
        verdict.deliver =
            Some(Delivery { meta: frame.meta, keyframe, headers: self.headers.take() });
        Ok(verdict)
    }

    fn bad(&mut self, bad: BadOutput, mut verdict: Verdict) -> Result<Verdict, OutputError> {
        self.stats.dropped += verdict.dropped.len() as u64;
        self.bad_outputs += 1;
        if self.bad_outputs >= MAX_BAD_OUTPUTS {
            return Err(OutputError::BadOutputs { count: self.bad_outputs, last: bad });
        }
        verdict.bad = Some(bad);
        Ok(verdict)
    }
}

struct H264Contents {
    idr: bool,
    /// A coded slice, not just parameter sets.
    picture: bool,
    /// Both SPS and PPS.
    parameter_sets: bool,
}

impl H264Contents {
    /// Returns `None` if `data` isn't Annex-B.
    fn of(data: &[u8]) -> Option<Self> {
        let nal_units = annex_b_nalus(data);
        if nal_units.is_empty() {
            return None;
        }
        let mut picture = false;
        for nal in &nal_units {
            picture |= (1..=5).contains(&h264_nal_type(nal).ok()?);
        }
        Some(Self {
            idr: h264_is_keyframe(&nal_units).ok()?,
            picture,
            parameter_sets: h264_has_parameter_sets(&nal_units).ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPS_PPS: [u8; 10] = [0, 0, 0, 1, 0x67, 1, 0, 0, 1, 0x68];
    const IDR: [u8; 6] = [0, 0, 0, 1, 0x65, 9];
    const DELTA: [u8; 6] = [0, 0, 0, 1, 0x61, 9];

    fn keyframe() -> Vec<u8> {
        [&SPS_PPS[..], &IDR].concat()
    }

    fn meta(rtp_timestamp: u32) -> FrameMeta {
        FrameMeta { rtp_timestamp, capture_time_ms: 0 }
    }

    fn encoded(timestamp: u64, data: &[u8]) -> Encoded<'_> {
        Encoded { data, timestamp, corrupt: false, keyframe: false }
    }

    fn tracker(frames: u32, now: Instant) -> Tracker {
        let mut tracker = Tracker::new(FourCc::H264, now);
        for frame in 1..=frames {
            tracker.queued(u64::from(frame), meta(frame), now);
        }
        tracker
    }

    fn delivered(rtp_timestamp: u32, keyframe: bool) -> Verdict {
        Verdict {
            deliver: Some(Delivery { meta: meta(rtp_timestamp), keyframe, headers: None }),
            ..Verdict::default()
        }
    }

    #[test]
    fn delivers_outputs_and_reads_keyframes_from_the_bitstream() {
        let now = Instant::now();
        let mut tracker = tracker(2, now);
        // The bitstream wins over the driver's flag.
        assert_eq!(tracker.output(&encoded(1, &keyframe()), now), Ok(delivered(1, true)));
        let flagged = Encoded { keyframe: true, ..encoded(2, &DELTA) };
        assert_eq!(tracker.output(&flagged, now), Ok(delivered(2, false)));
        assert_eq!(tracker.stats(), Stats { queued: 2, delivered: 2, dropped: 0 });
    }

    #[test]
    fn frames_the_encoder_skipped_are_dropped() {
        let now = Instant::now();
        let mut tracker = tracker(3, now);
        let verdict = tracker.output(&encoded(3, &keyframe()), now).unwrap();
        assert_eq!(verdict.dropped, [meta(1), meta(2)]);
        assert_eq!(verdict.deliver.unwrap().meta, meta(3));
        assert_eq!(tracker.stats().dropped, 2);
    }

    #[test]
    fn separate_headers_go_in_front_of_the_first_frame() {
        let now = Instant::now();
        let mut tracker = tracker(2, now);
        assert_eq!(tracker.output(&encoded(1, &SPS_PPS), now), Ok(Verdict::default()));
        let first = tracker.output(&encoded(1, &IDR), now).unwrap().deliver.unwrap();
        assert_eq!(
            first,
            Delivery { meta: meta(1), keyframe: true, headers: Some(SPS_PPS.into()) }
        );
        assert_eq!(tracker.output(&encoded(2, &DELTA), now), Ok(delivered(2, false)));
    }

    #[test]
    fn keyframe_without_parameter_sets_is_fatal() {
        let now = Instant::now();
        let mut tracker = tracker(1, now);
        assert_eq!(
            tracker.output(&encoded(1, &IDR), now),
            Err(OutputError::KeyframeWithoutParameterSets)
        );
    }

    #[test]
    fn nothing_is_delivered_before_the_first_keyframe() {
        let now = Instant::now();
        let mut tracker = tracker(STARTUP_KEYFRAME_FRAMES + 1, now);
        for frame in 1..=STARTUP_KEYFRAME_FRAMES {
            let verdict = tracker.output(&encoded(u64::from(frame), &DELTA), now).unwrap();
            assert_eq!(verdict, Verdict { dropped: vec![meta(frame)], ..Verdict::default() });
        }
        let late = u64::from(STARTUP_KEYFRAME_FRAMES + 1);
        assert_eq!(
            tracker.output(&encoded(late, &DELTA), now),
            Err(OutputError::NoKeyframe(STARTUP_KEYFRAME_FRAMES))
        );
    }

    #[test]
    fn a_requested_keyframe_must_arrive() {
        let now = Instant::now();
        let mut tracker = tracker(KEYFRAME_FRAMES + 3, now);
        tracker.output(&encoded(1, &keyframe()), now).unwrap();
        tracker.output(&encoded(2, &DELTA), now).unwrap();

        tracker.keyframe_requested();
        for frame in 3..KEYFRAME_FRAMES + 3 {
            let verdict = tracker.output(&encoded(u64::from(frame), &DELTA), now);
            assert_eq!(verdict, Ok(delivered(frame, false)));
        }
        let late = u64::from(KEYFRAME_FRAMES + 3);
        assert_eq!(
            tracker.output(&encoded(late, &DELTA), now),
            Err(OutputError::NoKeyframe(KEYFRAME_FRAMES))
        );
    }

    #[test]
    fn unusable_outputs_are_dropped_until_they_repeat() {
        let now = Instant::now();
        let mut tracker = tracker(MAX_BAD_OUTPUTS + 4, now);
        tracker.output(&encoded(1, &keyframe()), now).unwrap();

        let corrupt = Encoded { corrupt: true, ..encoded(2, &DELTA) };
        let cases = [
            (corrupt, BadOutput::Corrupt, vec![meta(2)]),
            (encoded(3, &[]), BadOutput::Empty, vec![meta(3)]),
            (encoded(4, &[1, 2, 3]), BadOutput::NotAnnexB, vec![meta(4)]),
            (encoded(999, &DELTA), BadOutput::Unmatched, vec![]),
        ];
        for (output, bad, dropped) in cases {
            let verdict = tracker.output(&output, now).unwrap();
            assert_eq!(verdict, Verdict { dropped, deliver: None, bad: Some(bad) });
        }

        assert_eq!(tracker.output(&encoded(5, &DELTA), now), Ok(delivered(5, false)));
        for frame in 6..MAX_BAD_OUTPUTS + 5 {
            assert!(tracker.output(&encoded(u64::from(frame), &[]), now).unwrap().bad.is_some());
        }
        assert_eq!(
            tracker.output(&encoded(u64::from(MAX_BAD_OUTPUTS + 5), &[]), now),
            Err(OutputError::BadOutputs { count: MAX_BAD_OUTPUTS, last: BadOutput::Unmatched })
        );
    }

    #[test]
    fn watchdog_fires_only_while_the_encoder_holds_frames() {
        let start = Instant::now();
        let late = start + WATCHDOG + Duration::from_millis(1);
        let mut tracker = Tracker::new(FourCc::H264, start);
        assert_eq!(tracker.deadline(), None);
        assert_eq!(tracker.check(late), Ok(()));

        tracker.queued(1, meta(1), late);
        assert_eq!(tracker.deadline(), Some(late + WATCHDOG));
        assert_eq!(tracker.check(late + WATCHDOG), Ok(()));
        let stalled = tracker.check(late + WATCHDOG + Duration::from_millis(1));
        assert!(matches!(stalled, Err(OutputError::Stalled(_))));

        tracker.output(&encoded(1, &keyframe()), late + WATCHDOG).unwrap();
        assert_eq!(tracker.check(late + 3 * WATCHDOG), Ok(()));
    }

    #[test]
    fn unparsed_formats_trust_the_driver() {
        let now = Instant::now();
        let mut tracker = Tracker::new(FourCc::new(b"FWHT"), now);
        tracker.queued(1, meta(1), now);
        let flagged = Encoded { keyframe: true, ..encoded(1, &[1, 2, 3]) };
        assert_eq!(tracker.output(&flagged, now), Ok(delivered(1, true)));
    }
}
