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

//! One encode session, following the kernel's `dev-encoder.rst`.

use std::{io, time::Duration};

use thiserror::Error;

use super::{
    device::EncoderDevice,
    sys::{self, Device, EventFd, Format, FourCc, Mapping, PlaneFormat, Queue, Readiness},
};
use crate::{Plane, I420};

// Requested; the driver may allocate more or fewer.
const OUTPUT_BUFFERS: u32 = 3;
const CAPTURE_BUFFERS: u32 = 4;

// From the kernel's `v4l2-controls.h`.
const CID_BITRATE: u32 = 0x0099_09cf;
const CID_FORCE_KEY_FRAME: u32 = 0x0099_09e5;
const CID_H264_PROFILE: u32 = 0x0099_0a6b;
const H264_PROFILE_CONSTRAINED_BASELINE: i32 = 1;

/// Best effort: a driver that refuses one keeps its default.
const OPTIONAL_CONTROLS: [(&str, u32, i32); 7] = [
    ("BITRATE_MODE=CBR", 0x0099_09ce, 1),
    ("FRAME_RC_ENABLE", 0x0099_09d7, 1),
    ("B_FRAMES=0", 0x0099_09ca, 0),
    // Keyframes come on request. 3000 is libwebrtc's own interval.
    ("GOP_SIZE", 0x0099_09cb, 3000),
    ("HEADER_MODE=JOINED_WITH_1ST_FRAME", 0x0099_09d8, 1),
    ("PREPEND_SPSPPS_TO_IDR", 0x0099_0b84, 1),
    ("REPEAT_SEQ_HEADER", 0x0099_09e2, 1),
];

#[derive(Clone, Debug)]
pub(crate) struct SessionConfig {
    pub coded: FourCc,
    pub width: u32,
    pub height: u32,
    pub bitrate_bps: u32,
    pub framerate: u32,
    /// Require bitrate control and forced keyframes, which WebRTC needs.
    pub strict: bool,
}

#[derive(Debug, Error)]
pub(crate) enum SetupError {
    #[error("{step} failed: {source}")]
    Os {
        step: String,
        #[source]
        source: io::Error,
    },
    #[error("{step}: {reason}")]
    Rejected { step: String, reason: String },
}

#[derive(Debug, Error)]
pub(crate) enum SessionError {
    #[error("{op} failed: {source}")]
    Os {
        op: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("a {width}x{height} frame does not fit the session's buffers")]
    Frame { width: u32, height: u32 },
    #[error("the driver returned buffer {index} with data outside its bounds")]
    Buffer { index: u32 },
}

pub(crate) struct Encoded<'a> {
    pub data: &'a [u8],
    /// The timestamp of the frame this was encoded from.
    pub timestamp: u64,
    /// Flagged by the driver.
    pub corrupt: bool,
    /// Flagged by the driver.
    pub keyframe: bool,
}

/// Dropping it releases everything, whatever state setup reached.
pub(crate) struct Session {
    layout: RawLayout,
    output: Vec<Vec<Mapping>>,
    capture: Vec<Vec<Mapping>>,
    free_output: Vec<u32>,
    /// Last, so it closes after the mappings are gone.
    device: Device,
}

impl Session {
    pub(crate) fn new(encoder: &EncoderDevice, config: &SessionConfig) -> Result<Self, SetupError> {
        let device = Device::open(&encoder.path).map_err(os("open"))?;
        let layout = negotiate(&device, encoder, config)?;
        apply_controls(&device, encoder, config)?;
        let mut session = Self {
            layout,
            output: Vec::new(),
            capture: Vec::new(),
            free_output: Vec::new(),
            device,
        };
        session.allocate()?;
        Ok(session)
    }

    fn allocate(&mut self) -> Result<(), SetupError> {
        self.output = self.map_queue(Queue::Output, OUTPUT_BUFFERS)?;
        self.capture = self.map_queue(Queue::Capture, CAPTURE_BUFFERS)?;
        self.free_output = (0..self.output.len() as u32).collect();
        for index in 0..self.capture.len() as u32 {
            self.device
                .queue_buffer(Queue::Capture, index, 0, &[])
                .map_err(os("VIDIOC_QBUF(Capture)"))?;
        }
        for queue in [Queue::Output, Queue::Capture] {
            self.device.stream_on(queue).map_err(os(format!("VIDIOC_STREAMON({queue:?})")))?;
        }
        Ok(())
    }

    fn map_queue(&self, queue: Queue, count: u32) -> Result<Vec<Vec<Mapping>>, SetupError> {
        let step = format!("VIDIOC_REQBUFS({queue:?})");
        let count = self.device.request_buffers(queue, count).map_err(os(step.clone()))?;
        if count == 0 {
            return Err(SetupError::Rejected {
                step,
                reason: "the driver allocated no buffers".into(),
            });
        }
        (0..count)
            .map(|index| {
                self.device
                    .map_buffer(queue, index)
                    .map_err(os(format!("mapping {queue:?} buffers")))
            })
            .collect()
    }

    pub(crate) fn free_buffers(&self) -> usize {
        self.free_output.len()
    }

    /// Returns `false` if no buffer is free.
    pub(crate) fn queue_frame(
        &mut self,
        frame: &I420<'_>,
        timestamp: u64,
    ) -> Result<bool, SessionError> {
        let Some(index) = self.free_output.pop() else {
            return Ok(false);
        };
        let queued = self.fill_and_queue(index, frame, timestamp);
        if queued.is_err() {
            self.free_output.push(index);
        }
        queued.map(|()| true)
    }

    fn fill_and_queue(
        &mut self,
        index: u32,
        frame: &I420<'_>,
        timestamp: u64,
    ) -> Result<(), SessionError> {
        let bad_frame = SessionError::Frame { width: frame.width, height: frame.height };
        let plane = self.output.get_mut(index as usize).and_then(|planes| planes.first_mut());
        let Some(plane) = plane else {
            return Err(bad_frame);
        };
        let Some(bytes_used) = self.layout.fill(frame, plane.as_mut_slice()) else {
            return Err(bad_frame);
        };
        self.device
            .queue_buffer(Queue::Output, index, timestamp, &[bytes_used as u32])
            .map_err(|source| SessionError::Os { op: "VIDIOC_QBUF(Output)", source })
    }

    pub(crate) fn reclaim(&mut self) -> Result<(), SessionError> {
        loop {
            let buffer = self
                .device
                .dequeue_buffer(Queue::Output)
                .map_err(|source| SessionError::Os { op: "VIDIOC_DQBUF(Output)", source })?;
            let Some(buffer) = buffer else {
                return Ok(());
            };
            self.free_output.push(buffer.index);
        }
    }

    pub(crate) fn dequeue<R>(
        &mut self,
        consume: impl FnOnce(Encoded<'_>) -> R,
    ) -> Result<Option<R>, SessionError> {
        let buffer = self
            .device
            .dequeue_buffer(Queue::Capture)
            .map_err(|source| SessionError::Os { op: "VIDIOC_DQBUF(Capture)", source })?;
        let Some(buffer) = buffer else {
            return Ok(None);
        };
        let data = self
            .capture
            .get(buffer.index as usize)
            .and_then(|planes| planes.first())
            .and_then(|plane| plane.as_slice().get(buffer.payload.clone()));
        let consumed = data.map(|data| {
            consume(Encoded {
                data,
                timestamp: buffer.timestamp,
                corrupt: buffer.error,
                keyframe: buffer.keyframe,
            })
        });
        self.device
            .queue_buffer(Queue::Capture, buffer.index, 0, &[])
            .map_err(|source| SessionError::Os { op: "VIDIOC_QBUF(Capture)", source })?;
        consumed.map(Some).ok_or(SessionError::Buffer { index: buffer.index })
    }

    pub(crate) fn wait(&self, wake: &EventFd, timeout: Duration) -> io::Result<Readiness> {
        sys::poll(&self.device, wake, timeout)
    }

    pub(crate) fn set_bitrate(&self, bitrate_bps: u32) -> io::Result<()> {
        self.device.set_control(CID_BITRATE, i32::try_from(bitrate_bps).unwrap_or(i32::MAX))
    }

    pub(crate) fn set_framerate(&self, framerate: u32) -> io::Result<()> {
        self.device.set_frame_interval(Queue::Output, 1, framerate.max(1))
    }

    pub(crate) fn force_keyframe(&self) -> io::Result<()> {
        self.device.set_control(CID_FORCE_KEY_FRAME, 0)
    }
}

impl Drop for Session {
    /// In the order the kernel requires. A failed step doesn't stop the rest.
    fn drop(&mut self) {
        for queue in [Queue::Output, Queue::Capture] {
            if let Err(error) = self.device.stream_off(queue) {
                log::warn!("V4L2 teardown: VIDIOC_STREAMOFF({queue:?}) failed: {error}");
            }
        }
        self.output.clear();
        self.capture.clear();
        for queue in [Queue::Output, Queue::Capture] {
            if let Err(error) = self.device.request_buffers(queue, 0) {
                log::warn!("V4L2 teardown: VIDIOC_REQBUFS({queue:?}, 0) failed: {error}");
            }
        }
    }
}

/// Checks what the driver chose rather than assuming it took what was asked.
fn negotiate(
    device: &Device,
    encoder: &EncoderDevice,
    config: &SessionConfig,
) -> Result<RawLayout, SetupError> {
    let format = |fourcc| Format {
        fourcc,
        width: config.width,
        height: config.height,
        planes: vec![PlaneFormat::default()],
    };

    // The coded format goes first: it constrains the raw one.
    let step = "VIDIOC_S_FMT(Capture)";
    let coded = device.set_format(Queue::Capture, &format(config.coded)).map_err(os(step))?;
    if coded.fourcc != config.coded {
        return Err(rejected(step, format!("the driver chose {}", coded.fourcc)));
    }

    let step = "VIDIOC_S_FMT(Output)";
    let raw = device.set_format(Queue::Output, &format(FourCc::NV12)).map_err(os(step))?;
    let [plane] = raw.planes[..] else {
        return Err(rejected(step, format!("the driver chose {} planes", raw.planes.len())));
    };
    let layout = RawLayout {
        width: config.width,
        height: config.height,
        stride: plane.stride as usize,
        rows: raw.height as usize,
        size: plane.size as usize,
    };
    let fits = raw.fourcc == FourCc::NV12
        && raw.width >= config.width
        && raw.height >= config.height
        && plane.stride >= raw.width
        && layout.needed() <= layout.size;
    if !fits {
        return Err(rejected(step, format!("the driver chose {raw:?}")));
    }

    if (raw.width, raw.height) != (config.width, config.height) {
        // The driver padded the frame; mark the picture so the stream has its real size.
        let step = "VIDIOC_S_SELECTION(Output)";
        let visible =
            device.set_crop(Queue::Output, config.width, config.height).map_err(os(step))?;
        if visible != (config.width, config.height) {
            return Err(rejected(step, format!("the driver chose {}x{}", visible.0, visible.1)));
        }
    }
    log::debug!("V4L2 {}: {} from {raw:?}", encoder.path.display(), config.coded);
    Ok(layout)
}

fn apply_controls(
    device: &Device,
    encoder: &EncoderDevice,
    config: &SessionConfig,
) -> Result<(), SetupError> {
    let mut unsupported = Vec::new();

    let bitrate = i32::try_from(config.bitrate_bps).unwrap_or(i32::MAX);
    if let Err(error) = device.set_control(CID_BITRATE, bitrate) {
        if config.strict {
            return Err(os("setting the bitrate")(error));
        }
        unsupported.push("BITRATE");
    }
    if !device.has_control(CID_FORCE_KEY_FRAME) {
        if config.strict {
            return Err(rejected("controls", "the driver cannot force a keyframe".into()));
        }
        unsupported.push("FORCE_KEY_FRAME");
    }

    let profile = (config.coded == FourCc::H264).then_some((
        "H264_PROFILE=CONSTRAINED_BASELINE",
        CID_H264_PROFILE,
        H264_PROFILE_CONSTRAINED_BASELINE,
    ));
    for (name, id, value) in OPTIONAL_CONTROLS.into_iter().chain(profile) {
        if device.set_control(id, value).is_err() {
            unsupported.push(name);
        }
    }
    if device.set_frame_interval(Queue::Output, 1, config.framerate.max(1)).is_err() {
        unsupported.push("frame interval");
    }

    if !unsupported.is_empty() {
        log::info!(
            "V4L2 {}: driver {} does not support: {}",
            encoder.path.display(),
            encoder.driver,
            unsupported.join(", ")
        );
    }
    Ok(())
}

fn os(step: impl Into<String>) -> impl FnOnce(io::Error) -> SetupError {
    let step = step.into();
    move |source| SetupError::Os { step, source }
}

fn rejected(step: &str, reason: String) -> SetupError {
    SetupError::Rejected { step: step.into(), reason }
}

/// An NV12 buffer as negotiated with the driver.
#[derive(Clone, Copy, Debug)]
struct RawLayout {
    width: u32,
    height: u32,
    stride: usize,
    /// Luma rows, padding included; chroma follows.
    rows: usize,
    size: usize,
}

impl RawLayout {
    fn chroma_size(&self) -> (usize, usize) {
        ((self.width as usize).div_ceil(2), (self.height as usize).div_ceil(2))
    }

    fn needed(&self) -> usize {
        let (_, chroma_rows) = self.chroma_size();
        self.stride * (self.rows + chroma_rows)
    }

    /// Returns `None` if the frame doesn't match the layout.
    // shortcut: no SIMD; use libyuv if profiling on a board shows it matters.
    fn fill(&self, frame: &I420<'_>, buffer: &mut [u8]) -> Option<usize> {
        if (frame.width, frame.height) != (self.width, self.height) {
            return None;
        }
        let (width, height) = (self.width as usize, self.height as usize);
        let (chroma_width, chroma_rows) = self.chroma_size();
        let (luma, chroma) =
            buffer.get_mut(..self.size)?.split_at_mut_checked(self.stride * self.rows)?;
        copy_rows(luma, self.stride, frame.y, width, height)?;
        for row in 0..chroma_rows {
            let u = row_of(frame.u, row, chroma_width)?;
            let v = row_of(frame.v, row, chroma_width)?;
            let out = chroma.get_mut(row * self.stride..)?.get_mut(..2 * chroma_width)?;
            for ((out, u), v) in out.chunks_exact_mut(2).zip(u).zip(v) {
                out[0] = *u;
                out[1] = *v;
            }
        }
        Some(self.size)
    }
}

fn row_of(plane: Plane<'_>, row: usize, width: usize) -> Option<&[u8]> {
    plane.data.get(row * plane.stride..)?.get(..width)
}

fn copy_rows(
    to: &mut [u8],
    to_stride: usize,
    from: Plane<'_>,
    width: usize,
    rows: usize,
) -> Option<()> {
    for row in 0..rows {
        to.get_mut(row * to_stride..)?.get_mut(..width)?.copy_from_slice(row_of(from, row, width)?);
    }
    Some(())
}

#[cfg_attr(not(feature = "__lk-vicodec-test"), allow(dead_code, unused_imports))]
#[cfg(test)]
pub(crate) mod testing {
    use super::{
        super::{
            device::{probe, EncoderDevice},
            sys::testing::{vicodec, FWHT},
        },
        SessionConfig,
    };
    use crate::{Plane, I420};

    pub(crate) const WIDTH: u32 = 640;
    /// vicodec pads it to 368, so every session exercises cropping.
    pub(crate) const HEIGHT: u32 = 360;

    pub(crate) fn encoder() -> EncoderDevice {
        probe(&vicodec(), FWHT).unwrap()
    }

    pub(crate) fn config() -> SessionConfig {
        SessionConfig {
            coded: FWHT,
            width: WIDTH,
            height: HEIGHT,
            bitrate_bps: 500_000,
            framerate: 30,
            strict: false,
        }
    }

    pub(crate) struct TestFrame {
        width: u32,
        height: u32,
        luma: Vec<u8>,
        chroma: Vec<u8>,
    }

    impl TestFrame {
        pub(crate) fn new(width: u32, height: u32) -> Self {
            let (w, h) = (width as usize, height as usize);
            Self {
                width,
                height,
                luma: vec![0x40; w * h],
                chroma: vec![0x80; w.div_ceil(2) * h.div_ceil(2)],
            }
        }

        pub(crate) fn i420(&self) -> I420<'_> {
            let chroma = Plane { data: &self.chroma, stride: (self.width as usize).div_ceil(2) };
            I420 {
                width: self.width,
                height: self.height,
                y: Plane { data: &self.luma, stride: self.width as usize },
                u: chroma,
                v: chroma,
            }
        }
    }
}

#[cfg_attr(not(feature = "__lk-vicodec-test"), allow(dead_code, unused_imports))]
#[cfg(test)]
mod tests {
    use super::{
        super::sys::{
            fault::{self, Action},
            testing::{exclusive, resources},
        },
        testing::{config, encoder, TestFrame, HEIGHT, WIDTH},
        *,
    };

    /// Padded with `0xee`, which must not be copied.
    const Y: [u8; 12] = [1, 2, 3, 4, 0xee, 0xee, 5, 6, 7, 8, 0xee, 0xee];
    const U: [u8; 3] = [10, 11, 0xee];
    const V: [u8; 3] = [20, 21, 0xee];

    fn small_frame() -> I420<'static> {
        I420 {
            width: 4,
            height: 2,
            y: Plane { data: &Y, stride: 6 },
            u: Plane { data: &U, stride: 3 },
            v: Plane { data: &V, stride: 3 },
        }
    }

    fn small_layout() -> RawLayout {
        let mut layout = RawLayout { width: 4, height: 2, stride: 8, rows: 4, size: 0 };
        layout.size = layout.needed();
        layout
    }

    #[test]
    fn fills_nv12_at_the_driver_stride() {
        let layout = small_layout();
        let mut buffer = vec![0; layout.size];
        assert_eq!(layout.fill(&small_frame(), &mut buffer), Some(40));
        #[rustfmt::skip]
        assert_eq!(buffer, [
            1, 2, 3, 4, 0, 0, 0, 0,
            5, 6, 7, 8, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
            10, 20, 11, 21, 0, 0, 0, 0,
        ]);
    }

    #[test]
    fn refuses_frames_that_do_not_fit() {
        let layout = small_layout();
        let mut buffer = vec![0; layout.size];

        let wrong_size = I420 { width: 2, ..small_frame() };
        assert_eq!(layout.fill(&wrong_size, &mut buffer), None);
        let short_plane = I420 { u: Plane { data: &U[..1], stride: 3 }, ..small_frame() };
        assert_eq!(layout.fill(&short_plane, &mut buffer), None);
        assert_eq!(layout.fill(&small_frame(), &mut buffer[..39]), None);
    }

    fn encode(session: &mut Session, count: u64) -> Vec<u64> {
        let wake = EventFd::new().unwrap();
        let frame = TestFrame::new(WIDTH, HEIGHT);
        let mut next = 1;
        let mut timestamps = Vec::new();
        while (timestamps.len() as u64) < count {
            session.reclaim().unwrap();
            while next <= count && session.queue_frame(&frame.i420(), next).unwrap() {
                next += 1;
            }
            let ready = session.wait(&wake, Duration::from_secs(5)).unwrap();
            assert!(!ready.error && (ready.capture || ready.output), "encoder stalled: {ready:?}");
            let timestamp = |encoded: Encoded<'_>| {
                assert!(!encoded.data.is_empty() && !encoded.corrupt);
                encoded.timestamp
            };
            while let Some(timestamp) = session.dequeue(timestamp).unwrap() {
                timestamps.push(timestamp);
            }
        }
        timestamps
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn encodes_each_frame_once_in_order() {
        let _exclusive = exclusive();
        let mut session = Session::new(&encoder(), &config()).unwrap();
        let free = session.free_buffers();
        assert!(free >= 1);

        let wrong_size = TestFrame::new(WIDTH / 2, HEIGHT);
        let error = session.queue_frame(&wrong_size.i420(), 0).unwrap_err();
        assert!(matches!(error, SessionError::Frame { .. }));
        assert_eq!(session.free_buffers(), free);

        assert_eq!(encode(&mut session, 12), (1..=12).collect::<Vec<u64>>());
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn sessions_release_everything() {
        let _exclusive = exclusive();
        let encoder = encoder();
        let baseline = resources(&encoder.path);
        let frame = TestFrame::new(WIDTH, HEIGHT);
        for cycle in 0..100 {
            let mut session = Session::new(&encoder, &config()).unwrap();
            let open = resources(&encoder.path);
            assert_eq!(open.device_fds, baseline.device_fds + 1);
            assert!(open.mappings > baseline.mappings);
            if cycle % 2 == 0 {
                encode(&mut session, 2);
            }
            while session.queue_frame(&frame.i420(), 100).unwrap() {}
        }
        assert_eq!(resources(&encoder.path), baseline);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn failed_setup_releases_everything() {
        let _exclusive = exclusive();
        let encoder = encoder();
        let baseline = resources(&encoder.path);
        let steps = [
            (fault::VIDIOC_S_FMT, 1, "VIDIOC_S_FMT(Capture)"),
            (fault::VIDIOC_S_FMT, 2, "VIDIOC_S_FMT(Output)"),
            (fault::VIDIOC_S_SELECTION, 1, "VIDIOC_S_SELECTION(Output)"),
            (fault::VIDIOC_REQBUFS, 1, "VIDIOC_REQBUFS(Output)"),
            (fault::VIDIOC_REQBUFS, 2, "VIDIOC_REQBUFS(Capture)"),
            (fault::VIDIOC_QUERYBUF, 2, "mapping Output buffers"),
            (fault::VIDIOC_QUERYBUF, 5, "mapping Capture buffers"),
            (fault::VIDIOC_QBUF, 2, "VIDIOC_QBUF(Capture)"),
            (fault::VIDIOC_STREAMON, 1, "VIDIOC_STREAMON(Output)"),
            (fault::VIDIOC_STREAMON, 2, "VIDIOC_STREAMON(Capture)"),
        ];
        for (request, nth, step) in steps {
            fault::inject(request, nth, Action::Fail(libc::EIO));
            let error = Session::new(&encoder, &config()).err().expect(step);
            assert_eq!(
                error.to_string(),
                format!("{step} failed: Input/output error (os error 5)")
            );
            assert_eq!(resources(&encoder.path), baseline, "after failing {step}");
        }
        assert_eq!(encode(&mut Session::new(&encoder, &config()).unwrap(), 1), [1]);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn strict_setup_needs_rate_control() {
        let _exclusive = exclusive();
        let encoder = encoder();
        let baseline = resources(&encoder.path);
        let strict = SessionConfig { strict: true, ..config() };
        let error = Session::new(&encoder, &strict).err().unwrap();
        assert!(error.to_string().starts_with("setting the bitrate failed"), "{error}");
        assert_eq!(resources(&encoder.path), baseline);
    }
}
