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

//! Safe wrappers over V4L2 ioctls, `mmap`, `eventfd` and `poll`: the crate's
//! only `unsafe` code.

mod bindings;

use std::{
    ffi::CStr,
    fmt,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    ops::Range,
    os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    ptr::{self, NonNull},
    time::Duration,
};

use bindings::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FourCc(u32);

impl FourCc {
    pub(crate) const NV12: Self = Self::new(b"NV12");
    pub(crate) const H264: Self = Self::new(b"H264");

    pub(crate) const fn new(code: &[u8; 4]) -> Self {
        Self(u32::from_le_bytes(*code))
    }
}

impl fmt::Display for FourCc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0.to_le_bytes() {
            let c = if byte.is_ascii_graphic() { byte as char } else { '?' };
            write!(f, "{c}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for FourCc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Queue {
    /// Raw frames in.
    Output,
    /// Encoded frames out.
    Capture,
}

impl Queue {
    fn buf_type(self) -> u32 {
        match self {
            Self::Output => V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            Self::Capture => V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Capability {
    pub driver: String,
    pub card: String,
    pub version: u32,
    /// A streaming, multi-planar memory-to-memory device.
    pub m2m_mplane: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PlaneFormat {
    /// `sizeimage`
    pub size: u32,
    /// `bytesperline`
    pub stride: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Format {
    pub fourcc: FourCc,
    pub width: u32,
    pub height: u32,
    pub planes: Vec<PlaneFormat>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Dequeued {
    pub index: u32,
    /// Copied by the driver from the raw frame to its output.
    pub timestamp: u64,
    /// Within the first plane.
    pub payload: Range<usize>,
    pub error: bool,
    pub keyframe: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Readiness {
    pub capture: bool,
    pub output: bool,
    pub error: bool,
    pub woken: bool,
}

pub(crate) struct Device {
    fd: OwnedFd,
}

impl Device {
    pub(crate) fn open(path: &Path) -> io::Result<Self> {
        let file =
            OpenOptions::new().read(true).write(true).custom_flags(libc::O_NONBLOCK).open(path)?;
        Ok(Self { fd: file.into() })
    }

    pub(crate) fn capability(&self) -> io::Result<Capability> {
        let mut cap: v4l2_capability = zeroed();
        // SAFETY: VIDIOC_QUERYCAP takes a `v4l2_capability`, which holds no pointers.
        unsafe { ioctl(self.fd.as_fd(), VIDIOC_QUERYCAP, &mut cap) }?;
        let caps = if cap.capabilities & V4L2_CAP_DEVICE_CAPS != 0 {
            cap.device_caps
        } else {
            cap.capabilities
        };
        let wanted = V4L2_CAP_VIDEO_M2M_MPLANE | V4L2_CAP_STREAMING;
        Ok(Capability {
            driver: string_from(&cap.driver),
            card: string_from(&cap.card),
            version: cap.version,
            m2m_mplane: caps & wanted == wanted,
        })
    }

    pub(crate) fn formats(&self, queue: Queue) -> io::Result<Vec<FourCc>> {
        // In case a driver never ends its list.
        const MAX_FORMATS: u32 = 256;
        let mut formats = Vec::new();
        for index in 0..MAX_FORMATS {
            let mut desc: v4l2_fmtdesc = zeroed();
            desc.index = index;
            desc.type_ = queue.buf_type();
            // SAFETY: VIDIOC_ENUM_FMT takes a `v4l2_fmtdesc`, which holds no pointers.
            match unsafe { ioctl(self.fd.as_fd(), VIDIOC_ENUM_FMT, &mut desc) } {
                Ok(()) => formats.push(FourCc(desc.pixelformat)),
                // EINVAL marks the end of the list.
                Err(error) if error.raw_os_error() == Some(libc::EINVAL) => break,
                Err(error) => return Err(error),
            }
        }
        Ok(formats)
    }

    /// Returns what the driver chose, which can differ in every field.
    pub(crate) fn set_format(&self, queue: Queue, wanted: &Format) -> io::Result<Format> {
        let mut pix_mp: v4l2_pix_format_mplane = zeroed();
        pix_mp.width = wanted.width;
        pix_mp.height = wanted.height;
        pix_mp.pixelformat = wanted.fourcc.0;
        pix_mp.field = V4L2_FIELD_NONE;
        let planes = &wanted.planes[..wanted.planes.len().min(VIDEO_MAX_PLANES)];
        pix_mp.num_planes = planes.len() as u8;
        for (index, plane) in planes.iter().enumerate() {
            pix_mp.plane_fmt[index].sizeimage = plane.size;
            pix_mp.plane_fmt[index].bytesperline = plane.stride;
        }
        let mut format: v4l2_format = zeroed();
        format.type_ = queue.buf_type();
        format.fmt.pix_mp = pix_mp;
        // SAFETY: VIDIOC_S_FMT takes a `v4l2_format`, which holds no pointers.
        unsafe { ioctl(self.fd.as_fd(), VIDIOC_S_FMT, &mut format) }?;
        Ok(read_format(&format))
    }

    /// Sets the visible size, anchored top left. Returns what the driver chose.
    pub(crate) fn set_crop(&self, queue: Queue, width: u32, height: u32) -> io::Result<(u32, u32)> {
        let mut selection: v4l2_selection = zeroed();
        selection.type_ = queue.buf_type();
        selection.target = V4L2_SEL_TGT_CROP;
        selection.width = width;
        selection.height = height;
        // SAFETY: VIDIOC_S_SELECTION takes a `v4l2_selection`, which holds no pointers.
        unsafe { ioctl(self.fd.as_fd(), VIDIOC_S_SELECTION, &mut selection) }?;
        Ok((selection.width, selection.height))
    }

    pub(crate) fn set_frame_interval(
        &self,
        queue: Queue,
        numerator: u32,
        denominator: u32,
    ) -> io::Result<()> {
        let mut parm: v4l2_streamparm = zeroed();
        parm.type_ = queue.buf_type();
        parm.timeperframe_numerator = numerator;
        parm.timeperframe_denominator = denominator;
        // SAFETY: VIDIOC_S_PARM takes a `v4l2_streamparm`, which holds no pointers.
        unsafe { ioctl(self.fd.as_fd(), VIDIOC_S_PARM, &mut parm) }
    }

    pub(crate) fn has_control(&self, id: u32) -> bool {
        let mut query: v4l2_queryctrl = zeroed();
        query.id = id;
        // SAFETY: VIDIOC_QUERYCTRL takes a `v4l2_queryctrl`, which holds no pointers.
        let result = unsafe { ioctl(self.fd.as_fd(), VIDIOC_QUERYCTRL, &mut query) };
        result.is_ok() && query.flags & V4L2_CTRL_FLAG_DISABLED == 0
    }

    pub(crate) fn set_control(&self, id: u32, value: i32) -> io::Result<()> {
        let mut control: v4l2_ext_control = zeroed();
        control.id = id;
        control.value = value;
        let mut controls: v4l2_ext_controls = zeroed();
        controls.count = 1;
        controls.controls = &mut control;
        // SAFETY: VIDIOC_S_EXT_CTRLS takes a `v4l2_ext_controls`. Its `controls`
        // pointer refers to `count` (one) live control that outlives the call.
        unsafe { ioctl(self.fd.as_fd(), VIDIOC_S_EXT_CTRLS, &mut controls) }
    }

    /// Returns how many buffers the driver allocated. Zero frees them, after
    /// their [`Mapping`]s are dropped.
    pub(crate) fn request_buffers(&self, queue: Queue, count: u32) -> io::Result<u32> {
        let mut request: v4l2_requestbuffers = zeroed();
        request.count = count;
        request.type_ = queue.buf_type();
        request.memory = V4L2_MEMORY_MMAP;
        // SAFETY: VIDIOC_REQBUFS takes a `v4l2_requestbuffers`, which holds no pointers.
        unsafe { ioctl(self.fd.as_fd(), VIDIOC_REQBUFS, &mut request) }?;
        Ok(request.count)
    }

    pub(crate) fn map_buffer(&self, queue: Queue, index: u32) -> io::Result<Vec<Mapping>> {
        let mut planes = [zeroed::<v4l2_plane>(); VIDEO_MAX_PLANES];
        let mut buffer = new_buffer(queue, index, &mut planes);
        // SAFETY: VIDIOC_QUERYBUF takes a `v4l2_buffer`. Its `m` pointer refers
        // to `length` live planes that outlive the call.
        unsafe { ioctl(self.fd.as_fd(), VIDIOC_QUERYBUF, &mut buffer) }?;
        let plane_count = (buffer.length as usize).min(VIDEO_MAX_PLANES);
        planes[..plane_count]
            .iter()
            .map(|plane| Mapping::new(self.fd.as_fd(), plane.mem_offset, plane.length as usize))
            .collect()
    }

    /// `bytes_used` is per plane, and ignored for [`Queue::Capture`].
    pub(crate) fn queue_buffer(
        &self,
        queue: Queue,
        index: u32,
        timestamp: u64,
        bytes_used: &[u32],
    ) -> io::Result<()> {
        let mut planes = [zeroed::<v4l2_plane>(); VIDEO_MAX_PLANES];
        for (plane, bytes_used) in planes.iter_mut().zip(bytes_used) {
            plane.bytesused = *bytes_used;
        }
        let mut buffer = new_buffer(queue, index, &mut planes);
        buffer.field = V4L2_FIELD_NONE;
        buffer.timestamp.tv_sec = (timestamp / 1_000_000) as libc::time_t;
        buffer.timestamp.tv_usec = (timestamp % 1_000_000) as libc::suseconds_t;
        // SAFETY: VIDIOC_QBUF takes a `v4l2_buffer`. Its `m` pointer refers to
        // `length` live planes that outlive the call.
        unsafe { ioctl(self.fd.as_fd(), VIDIOC_QBUF, &mut buffer) }
    }

    pub(crate) fn dequeue_buffer(&self, queue: Queue) -> io::Result<Option<Dequeued>> {
        let mut planes = [zeroed::<v4l2_plane>(); VIDEO_MAX_PLANES];
        let mut buffer = new_buffer(queue, 0, &mut planes);
        // SAFETY: VIDIOC_DQBUF takes a `v4l2_buffer`. Its `m` pointer refers to
        // `length` live planes that outlive the call.
        match unsafe { ioctl(self.fd.as_fd(), VIDIOC_DQBUF, &mut buffer) } {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(None),
            Err(error) => return Err(error),
        }
        let timestamp = buffer.timestamp;
        // `bytesused` includes `data_offset`.
        let end = planes[0].bytesused as usize;
        let start = (planes[0].data_offset as usize).min(end);
        Ok(Some(Dequeued {
            index: buffer.index,
            timestamp: (timestamp.tv_sec as u64) * 1_000_000 + timestamp.tv_usec as u64,
            payload: start..end,
            error: buffer.flags & V4L2_BUF_FLAG_ERROR != 0,
            keyframe: buffer.flags & V4L2_BUF_FLAG_KEYFRAME != 0,
        }))
    }

    pub(crate) fn stream_on(&self, queue: Queue) -> io::Result<()> {
        let mut buf_type = queue.buf_type() as i32;
        // SAFETY: VIDIOC_STREAMON takes an `int`.
        unsafe { ioctl(self.fd.as_fd(), VIDIOC_STREAMON, &mut buf_type) }
    }

    /// The driver returns every buffer of the queue.
    pub(crate) fn stream_off(&self, queue: Queue) -> io::Result<()> {
        let mut buf_type = queue.buf_type() as i32;
        // SAFETY: VIDIOC_STREAMOFF takes an `int`.
        unsafe { ioctl(self.fd.as_fd(), VIDIOC_STREAMOFF, &mut buf_type) }
    }
}

/// One plane of a driver buffer. Its contents are meaningful only while the
/// buffer is dequeued.
pub(crate) struct Mapping {
    ptr: NonNull<u8>,
    len: usize,
}

impl Mapping {
    fn new(fd: BorrowedFd<'_>, offset: u32, len: usize) -> io::Result<Self> {
        // SAFETY: a null hint can't replace an existing mapping, and `fd` is open.
        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                libc::off_t::from(offset),
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let ptr = NonNull::new(ptr.cast()).ok_or_else(|| io::Error::other("mmap returned null"))?;
        Ok(Self { ptr, len })
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        // SAFETY: `len` readable bytes stay mapped until `self` drops.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: `len` writable bytes stay mapped until `self` drops, and
        // `&mut self` makes this the only reference.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: as `mmap` returned them, and no borrowed slice outlives `self`.
        if unsafe { libc::munmap(self.ptr.as_ptr().cast(), self.len) } == -1 {
            log::warn!("munmap failed: {}", io::Error::last_os_error());
        }
    }
}

pub(crate) struct EventFd(File);

impl EventFd {
    pub(crate) fn new() -> io::Result<Self> {
        // SAFETY: `eventfd` takes no pointers.
        let fd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if fd == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` was just created and nothing else owns it.
        Ok(Self(unsafe { File::from_raw_fd(fd) }))
    }

    pub(crate) fn signal(&self) {
        // Fails only if a wake-up is already pending.
        let _ = (&self.0).write(&1u64.to_ne_bytes());
    }

    pub(crate) fn drain(&self) {
        // Fails only if nothing is pending.
        let _ = (&self.0).read(&mut [0; 8]);
    }
}

pub(crate) fn poll(device: &Device, wake: &EventFd, timeout: Duration) -> io::Result<Readiness> {
    let mut fds = [
        libc::pollfd {
            fd: device.fd.as_raw_fd(),
            events: libc::POLLIN | libc::POLLOUT,
            revents: 0,
        },
        libc::pollfd { fd: wake.0.as_raw_fd(), events: libc::POLLIN, revents: 0 },
    ];
    let timeout = timeout.as_millis().min(libc::c_int::MAX as u128) as libc::c_int;
    // SAFETY: `fds` has the length passed and both descriptors are open.
    let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) };
    if result == -1 {
        let error = io::Error::last_os_error();
        return if error.kind() == io::ErrorKind::Interrupted {
            Ok(Readiness::default())
        } else {
            Err(error)
        };
    }
    let device_events = fds[0].revents;
    Ok(Readiness {
        capture: device_events & libc::POLLIN != 0,
        output: device_events & libc::POLLOUT != 0,
        error: device_events & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0,
        woken: fds[1].revents != 0,
    })
}

/// # Safety
///
/// `request` must take a `T`, and any pointer in `arg` must be valid for it.
unsafe fn ioctl<T>(fd: BorrowedFd<'_>, request: libc::c_ulong, arg: &mut T) -> io::Result<()> {
    #[cfg(test)]
    fault::before_ioctl(request)?;
    loop {
        // SAFETY: `fd` is open and the caller guarantees `arg` matches `request`.
        let result = unsafe { libc::ioctl(fd.as_raw_fd(), request as libc::Ioctl, arg as *mut T) };
        if result != -1 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

fn zeroed<T: Zeroable>() -> T {
    // SAFETY: `Zeroable` types are valid when all zero.
    unsafe { std::mem::zeroed() }
}

/// Kernel structs of integers and nullable pointers, valid when all zero.
trait Zeroable {}
impl Zeroable for v4l2_capability {}
impl Zeroable for v4l2_fmtdesc {}
impl Zeroable for v4l2_pix_format_mplane {}
impl Zeroable for v4l2_format {}
impl Zeroable for v4l2_requestbuffers {}
impl Zeroable for v4l2_plane {}
impl Zeroable for v4l2_buffer {}
impl Zeroable for v4l2_ext_control {}
impl Zeroable for v4l2_ext_controls {}
impl Zeroable for v4l2_queryctrl {}
impl Zeroable for v4l2_streamparm {}
impl Zeroable for v4l2_selection {}

fn new_buffer(
    queue: Queue,
    index: u32,
    planes: &mut [v4l2_plane; VIDEO_MAX_PLANES],
) -> v4l2_buffer {
    let mut buffer: v4l2_buffer = zeroed();
    buffer.index = index;
    buffer.type_ = queue.buf_type();
    buffer.memory = V4L2_MEMORY_MMAP;
    buffer.m = planes.as_mut_ptr();
    buffer.length = VIDEO_MAX_PLANES as u32;
    buffer
}

fn read_format(format: &v4l2_format) -> Format {
    // SAFETY: any bit pattern is a valid `v4l2_pix_format_mplane`.
    let pix_mp = unsafe { format.fmt.pix_mp };
    let plane_fmt = pix_mp.plane_fmt;
    let planes = plane_fmt[..(pix_mp.num_planes as usize).min(VIDEO_MAX_PLANES)]
        .iter()
        .map(|plane| PlaneFormat { size: plane.sizeimage, stride: plane.bytesperline })
        .collect();
    Format {
        fourcc: FourCc(pix_mp.pixelformat),
        width: pix_mp.width,
        height: pix_mp.height,
        planes,
    }
}

fn string_from(bytes: &[u8]) -> String {
    CStr::from_bytes_until_nul(bytes)
        .map_or_else(|_| String::from_utf8_lossy(bytes), CStr::to_string_lossy)
        .into_owned()
}

/// Makes chosen ioctls fail, stall or panic, to reach error paths.
#[cfg_attr(not(feature = "__lk-vicodec-test"), allow(dead_code, unused_imports))]
#[cfg(test)]
pub(crate) mod fault {
    use std::{
        io,
        sync::{Mutex, PoisonError},
        time::Duration,
    };

    pub(crate) use super::bindings::{
        VIDIOC_DQBUF, VIDIOC_QBUF, VIDIOC_QUERYBUF, VIDIOC_REQBUFS, VIDIOC_STREAMON, VIDIOC_S_FMT,
        VIDIOC_S_SELECTION,
    };

    #[derive(Clone, Copy)]
    pub(crate) enum Action {
        Fail(i32),
        Stall(Duration),
        Panic,
    }

    struct Fault {
        request: libc::c_ulong,
        remaining: u32,
        action: Action,
        persistent: bool,
    }

    static FAULTS: Mutex<Vec<Fault>> = Mutex::new(Vec::new());

    /// Applies `action` once, to the `nth` call (from 1) of `request` on any thread.
    pub(crate) fn inject(request: libc::c_ulong, nth: u32, action: Action) {
        let fault = Fault { request, remaining: nth, action, persistent: false };
        FAULTS.lock().unwrap_or_else(PoisonError::into_inner).push(fault);
    }

    pub(crate) fn inject_always(request: libc::c_ulong, action: Action) {
        let fault = Fault { request, remaining: 1, action, persistent: true };
        FAULTS.lock().unwrap_or_else(PoisonError::into_inner).push(fault);
    }

    pub(crate) fn clear() {
        FAULTS.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }

    pub(super) fn before_ioctl(request: libc::c_ulong) -> io::Result<()> {
        let action = {
            let mut faults = FAULTS.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(index) = faults.iter().position(|fault| fault.request == request) else {
                return Ok(());
            };
            if faults[index].persistent {
                faults[index].action
            } else {
                faults[index].remaining -= 1;
                if faults[index].remaining > 0 {
                    return Ok(());
                }
                faults.remove(index).action
            }
        };
        match action {
            Action::Fail(errno) => Err(io::Error::from_raw_os_error(errno)),
            Action::Stall(duration) => {
                std::thread::sleep(duration);
                Ok(())
            }
            Action::Panic => panic!("injected fault"),
        }
    }
}

#[cfg_attr(not(feature = "__lk-vicodec-test"), allow(dead_code, unused_imports))]
#[cfg(test)]
pub(crate) mod testing {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{Mutex, MutexGuard, PoisonError},
    };

    use super::fault;

    pub(crate) const FWHT: super::FourCc = super::FourCc::new(b"FWHT");

    static LOCK: Mutex<()> = Mutex::new(());

    /// Serializes tests over process-wide state: faults and [`Resources`].
    pub(crate) struct Exclusive {
        _guard: MutexGuard<'static, ()>,
    }

    impl Drop for Exclusive {
        fn drop(&mut self) {
            fault::clear();
        }
    }

    pub(crate) fn exclusive() -> Exclusive {
        Exclusive { _guard: LOCK.lock().unwrap_or_else(PoisonError::into_inner) }
    }

    /// Panics if `vicodec` isn't loaded: a device test must not pass unrun.
    pub(crate) fn vicodec() -> PathBuf {
        let entries = fs::read_dir("/sys/class/video4linux").expect("no V4L2 devices");
        entries
            .filter_map(Result::ok)
            .find(|entry| {
                let name = fs::read_to_string(entry.path().join("name")).unwrap_or_default();
                name.trim() == "stateful-encoder"
            })
            .map(|entry| Path::new("/dev").join(entry.file_name()))
            .expect("vicodec is not loaded: run `sudo modprobe vicodec multiplanar=1`")
    }

    #[derive(Debug, PartialEq, Eq)]
    pub(crate) struct Resources {
        pub device_fds: usize,
        pub eventfds: usize,
        pub mappings: usize,
        pub threads: usize,
    }

    pub(crate) fn resources(device: &Path) -> Resources {
        let links: Vec<PathBuf> = fs::read_dir("/proc/self/fd")
            .expect("no procfs")
            .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
            .collect();
        let maps = fs::read_to_string("/proc/self/maps").expect("no procfs");
        let device_name = device.to_str().expect("device path is not UTF-8");
        let threads = fs::read_dir("/proc/self/task")
            .expect("no procfs")
            .filter_map(|entry| fs::read_to_string(entry.ok()?.path().join("comm")).ok())
            .filter(|name| name.trim() == super::super::THREAD_NAME)
            .count();
        Resources {
            device_fds: links.iter().filter(|link| *link == device).count(),
            eventfds: links
                .iter()
                .filter(|link| link.as_os_str() == "anon_inode:[eventfd]")
                .count(),
            mappings: maps.lines().filter(|line| line.ends_with(device_name)).count(),
            threads,
        }
    }
}

#[cfg_attr(not(feature = "__lk-vicodec-test"), allow(dead_code, unused_imports))]
#[cfg(test)]
mod tests {
    use super::{
        fault::{self, Action},
        testing::{exclusive, resources, vicodec, FWHT},
        *,
    };

    const WIDTH: u32 = 640;
    const HEIGHT: u32 = 360;

    fn format(fourcc: FourCc) -> Format {
        Format { fourcc, width: WIDTH, height: HEIGHT, planes: vec![PlaneFormat::default()] }
    }

    #[test]
    fn fourcc_displays_its_code() {
        assert_eq!(FourCc::NV12.to_string(), "NV12");
        assert_eq!(FourCc(0).to_string(), "????");
    }

    #[test]
    fn fault_applies_once_to_the_nth_call() {
        let _exclusive = exclusive();
        fault::inject(VIDIOC_QBUF, 2, Action::Fail(libc::ENODEV));
        assert!(fault::before_ioctl(VIDIOC_DQBUF).is_ok());
        assert!(fault::before_ioctl(VIDIOC_QBUF).is_ok());
        let error = fault::before_ioctl(VIDIOC_QBUF).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::ENODEV));
        assert!(fault::before_ioctl(VIDIOC_QBUF).is_ok());
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn vicodec_is_a_multiplanar_encoder() {
        let _exclusive = exclusive();
        let device = Device::open(&vicodec()).unwrap();
        let capability = device.capability().unwrap();
        assert_eq!(capability.driver, "vicodec");
        assert!(capability.m2m_mplane);
        assert_eq!(device.formats(Queue::Capture).unwrap(), [FWHT]);
        assert!(device.formats(Queue::Output).unwrap().contains(&FourCc::NV12));
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn injected_fault_fails_an_ioctl() {
        let _exclusive = exclusive();
        let device = Device::open(&vicodec()).unwrap();
        fault::inject(VIDIOC_QUERYCAP, 1, Action::Fail(libc::ENODEV));
        assert_eq!(device.capability().unwrap_err().raw_os_error(), Some(libc::ENODEV));
        assert!(device.capability().is_ok());
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn eventfd_wakes_poll() {
        let _exclusive = exclusive();
        let device = Device::open(&vicodec()).unwrap();
        let wake = EventFd::new().unwrap();
        assert!(!poll(&device, &wake, Duration::ZERO).unwrap().woken);
        wake.signal();
        wake.signal();
        assert!(poll(&device, &wake, Duration::from_secs(5)).unwrap().woken);
        wake.drain();
        assert!(!poll(&device, &wake, Duration::ZERO).unwrap().woken);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn buffers_map_and_unmap() {
        let _exclusive = exclusive();
        let path = vicodec();
        let baseline = resources(&path);
        for _ in 0..20 {
            let device = Device::open(&path).unwrap();
            device.set_format(Queue::Capture, &format(FWHT)).unwrap();
            let chosen = device.set_format(Queue::Output, &format(FourCc::NV12)).unwrap();
            // vicodec rounds the height up to a multiple of 8.
            assert!(chosen.width >= WIDTH && chosen.height >= HEIGHT);

            let count = device.request_buffers(Queue::Output, 2).unwrap();
            assert!(count >= 2);
            let mut buffers: Vec<Vec<Mapping>> =
                (0..count).map(|index| device.map_buffer(Queue::Output, index).unwrap()).collect();
            assert_eq!(resources(&path).mappings, count as usize * chosen.planes.len());
            for planes in &mut buffers {
                assert_eq!(planes.len(), chosen.planes.len());
                for (plane, plane_format) in planes.iter_mut().zip(&chosen.planes) {
                    assert!(plane.as_slice().len() >= plane_format.size as usize);
                    plane.as_mut_slice().fill(0x5a);
                    assert!(plane.as_slice().iter().all(|byte| *byte == 0x5a));
                }
            }

            drop(buffers);
            assert_eq!(device.request_buffers(Queue::Output, 0).unwrap(), 0);
        }
        assert_eq!(resources(&path), baseline);
    }

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn encodes_a_frame() {
        let _exclusive = exclusive();
        let device = Device::open(&vicodec()).unwrap();
        device.set_format(Queue::Capture, &format(FWHT)).unwrap();
        let raw = device.set_format(Queue::Output, &format(FourCc::NV12)).unwrap();

        let output_count = device.request_buffers(Queue::Output, 1).unwrap();
        let capture_count = device.request_buffers(Queue::Capture, 1).unwrap();
        let mut output: Vec<Vec<Mapping>> = (0..output_count)
            .map(|index| device.map_buffer(Queue::Output, index).unwrap())
            .collect();
        let capture: Vec<Vec<Mapping>> = (0..capture_count)
            .map(|index| device.map_buffer(Queue::Capture, index).unwrap())
            .collect();
        for index in 0..capture_count {
            device.queue_buffer(Queue::Capture, index, 0, &[]).unwrap();
        }
        output[0][0].as_mut_slice().fill(0x80);
        device.queue_buffer(Queue::Output, 0, 1_000_042, &[raw.planes[0].size]).unwrap();
        device.stream_on(Queue::Output).unwrap();
        device.stream_on(Queue::Capture).unwrap();

        let wake = EventFd::new().unwrap();
        let encoded = loop {
            let ready = poll(&device, &wake, Duration::from_secs(5)).unwrap();
            assert!(!ready.error && (ready.capture || ready.output), "encoder stalled: {ready:?}");
            if let Some(encoded) = device.dequeue_buffer(Queue::Capture).unwrap() {
                break encoded;
            }
        };
        assert_eq!(encoded.timestamp, 1_000_042);
        assert!(!encoded.error);
        let payload = &capture[encoded.index as usize][0].as_slice()[encoded.payload];
        assert!(!payload.is_empty());
        assert_eq!(device.dequeue_buffer(Queue::Capture).unwrap(), None);

        device.stream_off(Queue::Output).unwrap();
        device.stream_off(Queue::Capture).unwrap();
    }
}
