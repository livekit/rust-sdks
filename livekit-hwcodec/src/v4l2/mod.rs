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

//! Hardware encoding through V4L2 stateful encoders.

mod device;
mod output;
mod session;
#[allow(unsafe_code)]
mod sys;
mod worker;

use std::{
    mem,
    path::Path,
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc::{self, SyncSender, TrySendError},
        Arc, Mutex, MutexGuard, PoisonError,
    },
    thread::{self, JoinHandle},
};

use device::EncoderDevice;
use sys::{EventFd, FourCc};
use worker::{Job, Rates, Shared, Worker, MAX_IN_FLIGHT};

use crate::{Codec, EncodeStatus, EncoderConfig, Frame, FrameMeta, Sink};

// Linux truncates thread names past 15 bytes.
const THREAD_NAME: &str = "lk-v4l2-encoder";

/// Failed sessions after which a device is no longer used.
const BROKEN_AFTER: u32 = 3;

#[derive(Clone)]
struct Target {
    device: EncoderDevice,
    coded: FourCc,
    /// See [`session::SessionConfig::strict`].
    strict: bool,
    /// Shared by every encoder on the device.
    failures: Arc<AtomicU32>,
}

impl Target {
    fn new(device: EncoderDevice, coded: FourCc, strict: bool) -> Self {
        Self { device, coded, strict, failures: Arc::default() }
    }

    fn is_broken(&self) -> bool {
        self.failures.load(Ordering::Acquire) >= BROKEN_AFTER
    }
}

/// Creates [`V4l2Encoder`]s for the system's V4L2 encoders.
pub struct V4l2EncoderFactory {
    h264: Option<Target>,
    reaper: Arc<Reaper>,
}

impl V4l2EncoderFactory {
    /// Probes every V4L2 device node.
    pub fn probe() -> Self {
        let h264 = device::discover(FourCc::H264).into_iter().next();
        Self {
            h264: h264.map(|device| Target::new(device, FourCc::H264, true)),
            reaper: Arc::default(),
        }
    }

    /// Uses `path` as the H.264 encoder without probing it.
    pub fn with_device(path: &Path) -> Self {
        let device = EncoderDevice {
            path: path.to_owned(),
            driver: "unknown".into(),
            card: "unknown".into(),
            kernel: 0,
        };
        Self { h264: Some(Target::new(device, FourCc::H264, true)), reaper: Arc::default() }
    }

    /// The codecs a device was found for.
    pub fn codecs(&self) -> Vec<Codec> {
        self.h264.iter().map(|_| Codec::H264).collect()
    }

    /// Returns `None` if no device encodes `codec`, or if it has failed too
    /// often to be tried again.
    pub fn create<F: Frame>(&self, codec: Codec) -> Option<V4l2Encoder<F>> {
        let target = match codec {
            Codec::H264 => self.h264.clone()?,
        };
        (!target.is_broken()).then(|| V4l2Encoder::new(target, self.reaper.clone()))
    }
}

/// A V4L2 encoder. Each session runs on its own thread, so no method waits on
/// the driver.
pub struct V4l2Encoder<F: Frame> {
    target: Target,
    reaper: Arc<Reaper>,
    worker: Option<WorkerHandle<F>>,
    /// Stopped but not yet joined.
    retired: Option<JoinHandle<()>>,
}

struct WorkerHandle<F> {
    jobs: SyncSender<Job<F>>,
    shared: Arc<Shared>,
    thread: JoinHandle<()>,
}

impl<F: Frame> V4l2Encoder<F> {
    fn new(target: Target, reaper: Arc<Reaper>) -> Self {
        Self { target, reaper, worker: None, retired: None }
    }

    /// The kernel driver's name.
    pub fn driver(&self) -> &str {
        &self.target.device.driver
    }

    /// Starts a session that delivers to `sink`, ending the current one.
    /// Returns `false` only if its thread couldn't be started; a later
    /// failure makes [`V4l2Encoder::encode`] return [`EncodeStatus::Failed`].
    pub fn init<S: Sink>(&mut self, config: EncoderConfig, sink: S) -> bool {
        self.retire();
        let wake = match EventFd::new() {
            Ok(wake) => wake,
            Err(error) => {
                log::error!("V4L2: cannot create an eventfd: {error}");
                return false;
            }
        };
        let (jobs, queue) = mpsc::sync_channel(MAX_IN_FLIGHT);
        let shared = Arc::new(Shared::new(wake, self.retired.take()));
        let worker = Worker::new(queue, shared.clone(), sink, self.target.clone(), config);
        match thread::Builder::new().name(THREAD_NAME.into()).spawn(move || worker.run()) {
            Ok(thread) => {
                self.worker = Some(WorkerHandle { jobs, shared, thread });
                true
            }
            Err(error) => {
                log::error!("V4L2: cannot start an encoder thread: {error}");
                self.retired = lock(&shared.previous).take();
                false
            }
        }
    }

    /// Never waits: drops the frame if the encoder is full. A `keyframe`
    /// request outlives a dropped frame.
    pub fn encode(&mut self, frame: F, meta: FrameMeta, keyframe: bool) -> EncodeStatus {
        let Some(worker) = &self.worker else {
            return EncodeStatus::Failed;
        };
        let shared = &worker.shared;
        if shared.failed.load(Ordering::Acquire) {
            return EncodeStatus::Failed;
        }
        if keyframe {
            shared.keyframe.store(true, Ordering::Release);
        }
        if shared.in_flight.fetch_add(1, Ordering::AcqRel) >= MAX_IN_FLIGHT {
            shared.in_flight.fetch_sub(1, Ordering::AcqRel);
            return EncodeStatus::Dropped;
        }
        let status = match worker.jobs.try_send(Job { frame, meta }) {
            Ok(()) => {
                shared.wake.signal();
                return EncodeStatus::Queued;
            }
            Err(TrySendError::Full(_)) => EncodeStatus::Dropped,
            Err(TrySendError::Disconnected(_)) => EncodeStatus::Failed,
        };
        shared.in_flight.fetch_sub(1, Ordering::AcqRel);
        status
    }

    /// Sets the target bitrate and frame rate.
    pub fn set_rates(&self, bitrate_bps: u32, framerate: u32) {
        let Some(worker) = &self.worker else {
            return;
        };
        *lock(&worker.shared.rates) = Some(Rates { bitrate_bps, framerate });
        worker.shared.wake.signal();
    }

    /// Ends the session without waiting for it. The sink can still be called
    /// until the worker notices.
    pub fn release(&mut self) {
        self.retire();
    }

    fn retire(&mut self) {
        let Some(worker) = self.worker.take() else {
            return;
        };
        worker.shared.shutdown.store(true, Ordering::Release);
        worker.shared.wake.signal();
        if let Some(earlier) = self.retired.replace(worker.thread) {
            self.reaper.adopt(earlier);
        }
    }
}

impl<F: Frame> Drop for V4l2Encoder<F> {
    fn drop(&mut self) {
        self.retire();
        if let Some(thread) = self.retired.take() {
            self.reaper.adopt(thread);
        }
    }
}

/// Joins the threads of dropped encoders, so none is ever detached. Dropping
/// it waits for those still running.
#[derive(Default)]
struct Reaper {
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Reaper {
    /// Never waits: joins only threads that have finished.
    fn adopt(&self, thread: JoinHandle<()>) {
        let finished: Vec<JoinHandle<()>> = {
            let mut threads = lock(&self.threads);
            threads.push(thread);
            let (finished, running) =
                mem::take(&mut *threads).into_iter().partition(JoinHandle::is_finished);
            *threads = running;
            finished
        };
        for thread in finished {
            let _ = thread.join();
        }
    }
}

impl Drop for Reaper {
    fn drop(&mut self) {
        for thread in lock(&self.threads).drain(..) {
            let _ = thread.join();
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg_attr(not(feature = "__lk-vicodec-test"), allow(dead_code, unused_imports))]
#[cfg(test)]
mod tests {
    use std::{
        sync::mpsc::{Receiver, Sender},
        time::{Duration, Instant},
    };

    use super::{
        session::testing::{encoder, TestFrame, HEIGHT, WIDTH},
        sys::{
            fault::{self, Action},
            testing::{exclusive, resources, FWHT},
        },
        *,
    };
    use crate::I420;

    const PROMPT: Duration = Duration::from_millis(250);
    const TIMEOUT: Duration = Duration::from_secs(10);

    impl Frame for Arc<TestFrame> {
        fn with_i420<R>(&self, f: impl FnOnce(I420<'_>) -> R) -> Option<R> {
            Some(f(self.i420()))
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        Delivered { rtp_timestamp: u32, keyframe: bool },
        Dropped { rtp_timestamp: u32 },
    }

    struct Events(Sender<Event>);

    impl Sink for Events {
        fn deliver(&self, data: &[u8], meta: &FrameMeta, keyframe: bool) {
            assert!(!data.is_empty());
            let _ = self.0.send(Event::Delivered { rtp_timestamp: meta.rtp_timestamp, keyframe });
        }

        fn dropped(&self, meta: &FrameMeta) {
            let _ = self.0.send(Event::Dropped { rtp_timestamp: meta.rtp_timestamp });
        }
    }

    type TestEncoder = V4l2Encoder<Arc<TestFrame>>;

    fn config(width: u32, height: u32) -> EncoderConfig {
        EncoderConfig { width, height, bitrate_bps: 500_000, framerate: 30 }
    }

    fn meta(rtp_timestamp: u32) -> FrameMeta {
        FrameMeta { rtp_timestamp, capture_time_ms: 0 }
    }

    fn vicodec_encoder(reaper: &Arc<Reaper>) -> TestEncoder {
        V4l2Encoder::new(Target::new(encoder(), FWHT, false), reaper.clone())
    }

    fn start(encoder: &mut TestEncoder, width: u32, height: u32) -> Receiver<Event> {
        let (sender, events) = mpsc::channel();
        assert!(encoder.init(config(width, height), Events(sender)));
        events
    }

    /// Encodes frames `first..first + count` without dropping any.
    fn encode(
        encoder: &mut TestEncoder,
        events: &Receiver<Event>,
        frame: &Arc<TestFrame>,
        first: u32,
        count: u32,
    ) -> Vec<Event> {
        let mut seen = Vec::new();
        for rtp_timestamp in first..first + count {
            loop {
                match encoder.encode(frame.clone(), meta(rtp_timestamp), false) {
                    EncodeStatus::Queued => break,
                    EncodeStatus::Dropped => seen.push(events.recv_timeout(TIMEOUT).unwrap()),
                    EncodeStatus::Failed => panic!("encoder failed"),
                }
            }
        }
        let last = first + count - 1;
        while !seen.iter().any(
            |event| matches!(event, Event::Delivered { rtp_timestamp, .. } if *rtp_timestamp == last),
        ) {
            seen.push(events.recv_timeout(TIMEOUT).unwrap());
        }
        seen
    }

    fn wait_until_failed(encoder: &mut TestEncoder, frame: &Arc<TestFrame>) {
        let deadline = Instant::now() + TIMEOUT;
        while encoder.encode(frame.clone(), meta(0), false) != EncodeStatus::Failed {
            assert!(Instant::now() < deadline, "the encoder never reported its failure");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn delivers_every_frame_in_order() {
        let _exclusive = exclusive();
        let reaper = Arc::default();
        let mut encoder = vicodec_encoder(&reaper);
        let events = start(&mut encoder, WIDTH, HEIGHT);
        let frame = Arc::new(TestFrame::new(WIDTH, HEIGHT));

        // vicodec doesn't flag keyframes.
        let delivered = encode(&mut encoder, &events, &frame, 1, 30);
        let expected: Vec<Event> = (1..=30)
            .map(|rtp_timestamp| Event::Delivered { rtp_timestamp, keyframe: false })
            .collect();
        assert_eq!(delivered, expected);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn encoder_lifetimes_release_everything() {
        let _exclusive = exclusive();
        let device = encoder().path;
        let baseline = resources(&device);
        let frame = Arc::new(TestFrame::new(WIDTH, HEIGHT));
        let reaper = Arc::default();
        for _ in 0..200 {
            let mut encoder = vicodec_encoder(&reaper);
            let events = start(&mut encoder, WIDTH, HEIGHT);
            encode(&mut encoder, &events, &frame, 1, 3);
            encoder.release();
        }
        drop(reaper);
        assert_eq!(resources(&device), baseline);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn drop_without_release_releases_everything() {
        let _exclusive = exclusive();
        let device = encoder().path;
        let baseline = resources(&device);
        let frame = Arc::new(TestFrame::new(WIDTH, HEIGHT));
        let reaper = Arc::default();
        for cycle in 0..50 {
            let mut encoder = vicodec_encoder(&reaper);
            let events = start(&mut encoder, WIDTH, HEIGHT);
            if cycle % 2 == 0 {
                encode(&mut encoder, &events, &frame, 1, 2);
            }
            for rtp_timestamp in 10..20 {
                encoder.encode(frame.clone(), meta(rtp_timestamp), false);
            }
        }
        drop(reaper);
        assert_eq!(resources(&device), baseline);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn reinit_never_holds_two_sessions() {
        let _exclusive = exclusive();
        let device = encoder().path;
        let baseline = resources(&device);
        let reaper = Arc::default();
        let mut encoder = vicodec_encoder(&reaper);
        for cycle in 0..20 {
            let (width, height) = if cycle % 2 == 0 { (WIDTH, HEIGHT) } else { (1280, 720) };
            let frame = Arc::new(TestFrame::new(width, height));
            let events = start(&mut encoder, width, height);
            assert!(resources(&device).device_fds <= baseline.device_fds + 1);
            encode(&mut encoder, &events, &frame, 1, 3);
            let running = resources(&device);
            assert_eq!(running.device_fds, baseline.device_fds + 1);
            assert_eq!(running.threads, baseline.threads + 1);
        }
        drop(encoder);
        drop(reaper);
        assert_eq!(resources(&device), baseline);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn stalled_driver_never_blocks_the_caller() {
        let _exclusive = exclusive();
        let device = encoder().path;
        let baseline = resources(&device);
        let frame = Arc::new(TestFrame::new(WIDTH, HEIGHT));
        let reaper = Arc::default();
        let mut encoder = vicodec_encoder(&reaper);
        fault::inject(fault::VIDIOC_S_FMT, 1, Action::Stall(Duration::from_secs(2)));

        let started = Instant::now();
        let _events = start(&mut encoder, WIDTH, HEIGHT);
        let statuses: Vec<EncodeStatus> = (0..MAX_IN_FLIGHT as u32 + 2)
            .map(|rtp_timestamp| encoder.encode(frame.clone(), meta(rtp_timestamp), true))
            .collect();
        encoder.set_rates(250_000, 15);
        let shared = encoder.worker.as_ref().unwrap().shared.clone();
        encoder.release();
        drop(encoder);
        assert!(started.elapsed() < PROMPT, "calls took {:?}", started.elapsed());

        let mut expected = vec![EncodeStatus::Queued; MAX_IN_FLIGHT];
        expected.extend([EncodeStatus::Dropped; 2]);
        assert_eq!(statuses, expected);
        assert_eq!(shared.in_flight.load(Ordering::Acquire), MAX_IN_FLIGHT);
        assert!(shared.keyframe.load(Ordering::Acquire));
        drop(shared);

        drop(reaper);
        assert!(started.elapsed() >= Duration::from_secs(2));
        assert_eq!(resources(&device), baseline);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn failed_setup_makes_encode_fail() {
        let _exclusive = exclusive();
        let device = encoder().path;
        let baseline = resources(&device);
        let frame = Arc::new(TestFrame::new(WIDTH, HEIGHT));
        let reaper = Arc::default();
        let mut encoder = vicodec_encoder(&reaper);

        fault::inject(fault::VIDIOC_REQBUFS, 2, Action::Fail(libc::ENOMEM));
        let _events = start(&mut encoder, WIDTH, HEIGHT);
        wait_until_failed(&mut encoder, &frame);

        let events = start(&mut encoder, WIDTH, HEIGHT);
        assert_eq!(encode(&mut encoder, &events, &frame, 1, 2).len(), 2);
        drop(encoder);
        drop(reaper);
        assert_eq!(resources(&device), baseline);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn stalled_encoder_trips_the_watchdog() {
        let _exclusive = exclusive();
        let device = encoder().path;
        let baseline = resources(&device);
        let frame = Arc::new(TestFrame::new(WIDTH, HEIGHT));
        let reaper = Arc::default();
        let mut encoder = vicodec_encoder(&reaper);

        fault::inject_always(fault::VIDIOC_DQBUF, Action::Fail(libc::EAGAIN));
        let started = Instant::now();
        let _events = start(&mut encoder, WIDTH, HEIGHT);
        wait_until_failed(&mut encoder, &frame);
        assert!(started.elapsed() >= output::WATCHDOG);

        drop(encoder);
        drop(reaper);
        assert_eq!(resources(&device), baseline);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn device_error_mid_stream_fails_the_encoder() {
        let _exclusive = exclusive();
        let device = encoder().path;
        let baseline = resources(&device);
        let frame = Arc::new(TestFrame::new(WIDTH, HEIGHT));
        let reaper = Arc::default();
        let mut encoder = vicodec_encoder(&reaper);
        let events = start(&mut encoder, WIDTH, HEIGHT);
        encode(&mut encoder, &events, &frame, 1, 2);

        fault::inject(fault::VIDIOC_DQBUF, 1, Action::Fail(libc::ENODEV));
        wait_until_failed(&mut encoder, &frame);

        drop(encoder);
        drop(reaper);
        assert_eq!(resources(&device), baseline);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn panic_in_the_worker_is_contained() {
        let _exclusive = exclusive();
        let device = encoder().path;
        let baseline = resources(&device);
        let frame = Arc::new(TestFrame::new(WIDTH, HEIGHT));
        let reaper = Arc::default();
        let mut encoder = vicodec_encoder(&reaper);
        let events = start(&mut encoder, WIDTH, HEIGHT);
        encode(&mut encoder, &events, &frame, 1, 2);

        fault::inject(fault::VIDIOC_QBUF, 1, Action::Panic);
        wait_until_failed(&mut encoder, &frame);

        drop(encoder);
        drop(reaper);
        assert_eq!(resources(&device), baseline);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn device_is_abandoned_after_repeated_failures() {
        let _exclusive = exclusive();
        let frame = Arc::new(TestFrame::new(WIDTH, HEIGHT));
        let factory = V4l2EncoderFactory {
            h264: Some(Target::new(encoder(), FWHT, false)),
            reaper: Arc::default(),
        };
        for _ in 0..BROKEN_AFTER {
            let mut encoder: TestEncoder = factory.create(Codec::H264).unwrap();
            fault::inject(fault::VIDIOC_S_FMT, 1, Action::Fail(libc::EIO));
            let _events = start(&mut encoder, WIDTH, HEIGHT);
            wait_until_failed(&mut encoder, &frame);
        }
        assert!(factory.create::<Arc<TestFrame>>(Codec::H264).is_none());
        assert_eq!(factory.codecs(), [Codec::H264]);
    }

    #[test]
    fn missing_device_makes_encode_fail() {
        let _exclusive = exclusive();
        let factory = V4l2EncoderFactory::with_device(Path::new("/nonexistent/video0"));
        let frame = Arc::new(TestFrame::new(WIDTH, HEIGHT));
        let mut encoder: TestEncoder = factory.create(Codec::H264).unwrap();
        assert_eq!(encoder.driver(), "unknown");

        assert_eq!(encoder.encode(frame.clone(), meta(0), false), EncodeStatus::Failed);
        let _events = start(&mut encoder, WIDTH, HEIGHT);
        wait_until_failed(&mut encoder, &frame);
    }
}
