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
    any::Any,
    io,
    panic::{self, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::Receiver,
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use thiserror::Error;

use super::{
    lock,
    output::{OutputError, Tracker},
    session::{Encoded, Session, SessionConfig, SessionError, SetupError},
    sys::EventFd,
    Target, BROKEN_AFTER,
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
    #[error("{0} failed: {1}")]
    Control(&'static str, #[source] io::Error),
    #[error("unusable output: {0}")]
    Output(#[from] OutputError),
}

pub(super) struct Worker<F, S> {
    jobs: Receiver<Job<F>>,
    shared: Arc<Shared>,
    sink: S,
    target: Target,
    config: EncoderConfig,
    tracker: Tracker,
    started: Instant,
    last_timestamp: u64,
    bad_outputs: u64,
}

impl<F: Frame, S: Sink> Worker<F, S> {
    pub(super) fn new(
        jobs: Receiver<Job<F>>,
        shared: Arc<Shared>,
        sink: S,
        target: Target,
        config: EncoderConfig,
    ) -> Self {
        let started = Instant::now();
        Self {
            jobs,
            shared,
            sink,
            tracker: Tracker::new(target.coded, started),
            target,
            config,
            started,
            last_timestamp: 0,
            bad_outputs: 0,
        }
    }

    /// The one place a failure is flagged, reported and counted against the device.
    pub(super) fn run(mut self) {
        if let Some(previous) = lock(&self.shared.previous).take() {
            let _ = previous.join();
        }
        let error = match panic::catch_unwind(AssertUnwindSafe(|| self.encode())) {
            Ok(Ok(())) => return,
            Ok(Err(error)) => error.to_string(),
            Err(panic) => format!("the encoder thread panicked: {}", panic_message(panic.as_ref())),
        };
        self.shared.failed.store(true, Ordering::Release);
        let device = &self.target.device;
        log::error!(
            "V4L2 {device}: {} {}x{} encoder failed: {error} ({})",
            self.target.coded,
            self.config.width,
            self.config.height,
            self.tracker.stats()
        );
        if self.target.failures.fetch_add(1, Ordering::AcqRel) + 1 == BROKEN_AFTER {
            log::error!(
                "V4L2 {device}: {BROKEN_AFTER} sessions have failed; \
                 not using this encoder again in this process"
            );
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
            self.apply_rates(&session)?;
            session.reclaim()?;
            self.queue_frames(&mut session)?;
            while let Some(delivered) = session.dequeue(|encoded| self.deliver(encoded))? {
                delivered?;
            }

            let now = Instant::now();
            self.tracker.check(now)?;
            let timeout = self.tracker.deadline().map_or(IDLE_WAIT, |deadline| {
                deadline.saturating_duration_since(now) + Duration::from_millis(1)
            });
            let ready = session.wait(&self.shared.wake, timeout).map_err(WorkerError::Poll)?;
            if ready.error {
                return Err(WorkerError::Device);
            }
            if ready.woken {
                self.shared.wake.drain();
            }
        }
        Ok(())
    }

    fn apply_rates(&self, session: &Session) -> Result<(), WorkerError> {
        let Some(rates) = lock(&self.shared.rates).take() else {
            return Ok(());
        };
        // Zero means paused; no frames will come.
        if rates.bitrate_bps > 0 {
            self.required("setting the bitrate", session.set_bitrate(rates.bitrate_bps))?;
        }
        if rates.framerate > 0 {
            // Optional; setup already logged whether the driver supports it.
            let _ = session.set_framerate(rates.framerate);
        }
        Ok(())
    }

    /// Only strict sessions require these controls.
    fn required(&self, name: &'static str, result: io::Result<()>) -> Result<(), WorkerError> {
        match result {
            Err(error) if self.target.strict => Err(WorkerError::Control(name, error)),
            _ => Ok(()),
        }
    }

    fn queue_frames(&mut self, session: &mut Session) -> Result<(), WorkerError> {
        while session.free_buffers() > 0 {
            // The channel disconnects only after `shutdown` is set.
            let Ok(job) = self.jobs.try_recv() else {
                break;
            };
            if self.shared.keyframe.swap(false, Ordering::AcqRel) {
                self.required("forcing a keyframe", session.force_keyframe())?;
                self.tracker.keyframe_requested();
            }
            let timestamp = self.next_timestamp();
            match job.frame.with_i420(|frame| session.queue_frame(&frame, timestamp)) {
                Some(Ok(true)) => self.tracker.queued(timestamp, job.meta, Instant::now()),
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

    fn deliver(&mut self, encoded: Encoded<'_>) -> Result<(), OutputError> {
        let verdict = self.tracker.output(&encoded, Instant::now())?;
        for meta in &verdict.dropped {
            self.drop_frame(meta);
        }
        if let Some(bad) = verdict.bad {
            self.bad_outputs += 1;
            if self.bad_outputs.is_power_of_two() {
                log::warn!("V4L2: unusable output ({bad}); {} so far", self.bad_outputs);
            }
            // The stream now has a gap.
            self.shared.keyframe.store(true, Ordering::Release);
        }
        if let Some(delivery) = verdict.deliver {
            match delivery.headers {
                Some(headers) => {
                    let joined = [&headers[..], encoded.data].concat();
                    self.sink.deliver(&joined, &delivery.meta, delivery.keyframe);
                }
                None => self.sink.deliver(encoded.data, &delivery.meta, delivery.keyframe),
            }
            self.shared.in_flight.fetch_sub(1, Ordering::AcqRel);
        }
        Ok(())
    }

    fn drop_frame(&self, meta: &FrameMeta) {
        self.sink.dropped(meta);
        self.shared.in_flight.fetch_sub(1, Ordering::AcqRel);
    }
}

fn panic_message(panic: &(dyn Any + Send)) -> &str {
    if let Some(message) = panic.downcast_ref::<&str>() {
        message
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message
    } else {
        "unknown"
    }
}
