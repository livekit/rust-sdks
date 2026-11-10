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

//! The thread that owns a [`Session`], so that only it waits on the driver.

use std::{
    collections::VecDeque,
    io,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::Receiver,
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use livekit_bitstream::{annex_b_nalus, h264_is_keyframe};
use thiserror::Error;

use super::{
    lock,
    session::{Encoded, Session, SessionConfig, SessionError, SetupError},
    sys::{EventFd, FourCc},
    Target,
};
use crate::{EncoderConfig, Frame, FrameMeta, Sink};

/// Frames accepted but not yet delivered. Past this, frames are dropped
/// rather than queued.
pub(super) const MAX_IN_FLIGHT: usize = 3;

const IDLE_WAIT: Duration = Duration::from_secs(1);

pub(super) struct Job<F> {
    pub frame: F,
    pub meta: FrameMeta,
}

#[derive(Clone, Copy)]
pub(super) struct Rates {
    pub bitrate_bps: u32,
    pub framerate: u32,
}

/// State shared by an encoder and its worker.
pub(super) struct Shared {
    /// Signalled after any other field changes.
    pub wake: EventFd,
    pub shutdown: AtomicBool,
    pub failed: AtomicBool,
    /// A pending keyframe request, kept here so it outlives a dropped frame.
    pub keyframe: AtomicBool,
    pub in_flight: AtomicUsize,
    pub rates: Mutex<Option<Rates>>,
    /// Joined before the device is opened, so an encoder never holds two sessions.
    pub previous: Mutex<Option<JoinHandle<()>>>,
}

impl Shared {
    pub(super) fn new(wake: EventFd, previous: Option<JoinHandle<()>>) -> Self {
        Self {
            wake,
            shutdown: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            keyframe: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
            rates: Mutex::new(None),
            previous: Mutex::new(previous),
        }
    }
}

#[derive(Debug, Error)]
enum WorkerError {
    #[error("session setup failed: {0}")]
    Setup(#[from] SetupError),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error("poll failed: {0}")]
    Poll(#[source] io::Error),
    #[error("the device reported an error condition")]
    Device,
}

struct Pending {
    timestamp: u64,
    meta: FrameMeta,
}

pub(super) struct Worker<F, S> {
    jobs: Receiver<Job<F>>,
    shared: Arc<Shared>,
    sink: S,
    target: Target,
    config: EncoderConfig,
    pending: VecDeque<Pending>,
    started: Instant,
    last_timestamp: u64,
}

impl<F: Frame, S: Sink> Worker<F, S> {
    pub(super) fn new(
        jobs: Receiver<Job<F>>,
        shared: Arc<Shared>,
        sink: S,
        target: Target,
        config: EncoderConfig,
    ) -> Self {
        Self {
            jobs,
            shared,
            sink,
            target,
            config,
            pending: VecDeque::new(),
            started: Instant::now(),
            last_timestamp: 0,
        }
    }

    /// The one place a failure is flagged and reported.
    pub(super) fn run(mut self) {
        if let Some(previous) = lock(&self.shared.previous).take() {
            let _ = previous.join();
        }
        if let Err(error) = self.encode() {
            log::error!("V4L2 {}: encoder stopped: {error}", self.target.device);
            self.shared.failed.store(true, Ordering::Release);
        }
    }

    fn encode(&mut self) -> Result<(), WorkerError> {
        let mut session = Session::new(
            &self.target.device,
            &SessionConfig {
                coded: self.target.coded,
                width: self.config.width,
                height: self.config.height,
                bitrate_bps: self.config.bitrate_bps,
                framerate: self.config.framerate,
                strict: self.target.strict,
            },
        )?;
        while !self.shared.shutdown.load(Ordering::Acquire) {
            self.apply_rates(&session);
            session.reclaim()?;
            self.queue_frames(&mut session)?;
            while session.dequeue(|encoded| self.deliver(encoded))?.is_some() {}

            let ready = session.wait(&self.shared.wake, IDLE_WAIT).map_err(WorkerError::Poll)?;
            if ready.error {
                return Err(WorkerError::Device);
            }
            if ready.woken {
                self.shared.wake.drain();
            }
        }
        Ok(())
    }

    fn apply_rates(&self, session: &Session) {
        let Some(rates) = lock(&self.shared.rates).take() else {
            return;
        };
        // Zero means paused; no frames will come.
        if rates.bitrate_bps > 0 {
            if let Err(error) = session.set_bitrate(rates.bitrate_bps) {
                log::warn!("V4L2: setting bitrate to {} failed: {error}", rates.bitrate_bps);
            }
        }
        if rates.framerate > 0 {
            // Optional; setup already logged whether the driver supports it.
            let _ = session.set_framerate(rates.framerate);
        }
    }

    fn queue_frames(&mut self, session: &mut Session) -> Result<(), WorkerError> {
        while session.free_buffers() > 0 {
            // The channel disconnects only after `shutdown` is set.
            let Ok(job) = self.jobs.try_recv() else {
                break;
            };
            if self.shared.keyframe.swap(false, Ordering::AcqRel) {
                if let Err(error) = session.force_keyframe() {
                    log::warn!("V4L2: forcing a keyframe failed: {error}");
                }
            }
            let timestamp = self.next_timestamp();
            match job.frame.with_i420(|frame| session.queue_frame(&frame, timestamp)) {
                Some(Ok(true)) => self.pending.push_back(Pending { timestamp, meta: job.meta }),
                Some(Err(error @ SessionError::Os { .. })) => return Err(error.into()),
                unusable => {
                    if let Some(Err(error)) = unusable {
                        log::warn!("V4L2: dropping frame: {error}");
                    }
                    self.drop_frame(&job.meta);
                }
            }
        }
        Ok(())
    }

    /// Matches outputs to frames. Real microseconds, not a counter, in case a
    /// driver paces rate control by them.
    fn next_timestamp(&mut self) -> u64 {
        let now = self.started.elapsed().as_micros() as u64;
        self.last_timestamp = now.max(self.last_timestamp + 1);
        self.last_timestamp
    }

    fn deliver(&mut self, encoded: Encoded<'_>) {
        let position = self.pending.iter().position(|frame| frame.timestamp == encoded.timestamp);
        let Some(position) = position else {
            log::warn!("V4L2: output with timestamp {} matches no frame", encoded.timestamp);
            return;
        };
        // The encoder skipped the frames before this one.
        for _ in 0..position {
            if let Some(skipped) = self.pending.pop_front() {
                self.drop_frame(&skipped.meta);
            }
        }
        let Some(frame) = self.pending.pop_front() else {
            return;
        };
        if encoded.corrupt {
            self.drop_frame(&frame.meta);
            return;
        }
        let keyframe = is_keyframe(self.target.coded, &encoded);
        self.sink.deliver(encoded.data, &frame.meta, keyframe);
        self.shared.in_flight.fetch_sub(1, Ordering::AcqRel);
    }

    fn drop_frame(&self, meta: &FrameMeta) {
        self.sink.dropped(meta);
        self.shared.in_flight.fetch_sub(1, Ordering::AcqRel);
    }
}

/// The driver's flag is used only for bitstreams this crate can't parse.
fn is_keyframe(coded: FourCc, encoded: &Encoded<'_>) -> bool {
    if coded == FourCc::H264 {
        h264_is_keyframe(&annex_b_nalus(encoded.data)).unwrap_or(false)
    } else {
        encoded.keyframe
    }
}
