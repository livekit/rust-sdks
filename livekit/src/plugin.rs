use std::{
    ffi::{c_char, c_void, CString},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use futures_util::Stream;
use libloading::{Library, Symbol};
use libwebrtc::{audio_stream::native::NativeAudioStream, prelude::AudioFrame};

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("dylib error: {0}")]
    Library(#[from] libloading::Error),
}

type OnLoadFn = unsafe extern "C" fn(options: *const c_char);
type CreateFn = unsafe extern "C" fn(sampling_rate: u32, options: *const c_char) -> *mut c_void;
type DestroyFn = unsafe extern "C" fn(*const c_void);
type ProcessI16Fn = unsafe extern "C" fn(*const c_void, usize, *const i16, *mut i16);
type ProcessF32Fn = unsafe extern "C" fn(*const c_void, usize, *const f32, *mut f32);

pub struct AudioFilterPlugin {
    lib: Library,
    dependencies: Vec<Library>,
    create_fn_ptr: *const c_void,
    destroy_fn_ptr: *const c_void,
    process_i16_fn_ptr: *const c_void,
    process_f32_fn_ptr: *const c_void,
}

impl AudioFilterPlugin {
    pub fn new<P: AsRef<str>>(path: P, options: P) -> Result<Arc<Self>, PluginError> {
        Ok(Arc::new(Self::_new(path, options)?))
    }

    pub fn new_with_dependencies<P: AsRef<str>>(
        path: P,
        dependencies: Vec<P>,
        options: P,
    ) -> Result<Arc<Self>, PluginError> {
        let mut libs = vec![];
        for path in dependencies {
            let lib = unsafe { Library::new(path.as_ref()) }?;
            libs.push(lib);
        }
        let mut this = Self::_new(path, options)?;
        this.dependencies = libs;
        Ok(Arc::new(this))
    }

    fn _new<P: AsRef<str>>(path: P, options: P) -> Result<Self, PluginError> {
        let lib = unsafe { Library::new(path.as_ref()) }?;

        let options = CString::new(options.as_ref()).unwrap_or(CString::new("").unwrap());
        unsafe {
            let on_load = lib.get::<Symbol<OnLoadFn>>(b"on_load")?;
            on_load(options.as_ptr());
        };

        let create_fn_ptr = unsafe {
            lib.get::<Symbol<CreateFn>>(b"audio_filter_create")?.try_as_raw_ptr().unwrap()
        };
        let destroy_fn_ptr = unsafe {
            lib.get::<Symbol<DestroyFn>>(b"audio_filter_destroy")?.try_as_raw_ptr().unwrap()
        };
        let process_i16_fn_ptr = unsafe {
            lib.get::<Symbol<ProcessI16Fn>>(b"audio_filter_process_int16")?
                .try_as_raw_ptr()
                .unwrap()
        };
        let process_f32_fn_ptr = unsafe {
            lib.get::<Symbol<ProcessF32Fn>>(b"audio_filter_process_float")?
                .try_as_raw_ptr()
                .unwrap()
        };

        Ok(Self {
            lib,
            dependencies: Default::default(),
            create_fn_ptr,
            destroy_fn_ptr,
            process_i16_fn_ptr,
            process_f32_fn_ptr,
        })
    }

    pub fn new_session<S: AsRef<str>>(
        self: Arc<Self>,
        sampling_rate: u32,
        options: S,
    ) -> AudioFilterSession {
        let create_fn: CreateFn = unsafe { std::mem::transmute(self.create_fn_ptr) };

        let options = CString::new(options.as_ref()).unwrap_or(CString::new("").unwrap());
        let ptr = unsafe { create_fn(sampling_rate, options.as_ptr()) };

        AudioFilterSession { plugin: self.clone(), ptr }
    }
}

pub struct AudioFilterSession {
    plugin: Arc<AudioFilterPlugin>,
    ptr: *const c_void,
}

impl AudioFilterSession {
    pub fn destroy(&self) {
        let destroy: DestroyFn = unsafe { std::mem::transmute(self.plugin.destroy_fn_ptr) };
        unsafe { destroy(self.ptr) };
    }

    pub fn process_i16(&self, num_samples: usize, input: &[i16], output: &mut [i16]) {
        let process: ProcessI16Fn = unsafe { std::mem::transmute(self.plugin.process_i16_fn_ptr) };
        unsafe { process(self.ptr, num_samples, input.as_ptr(), output.as_mut_ptr()) };
    }

    pub fn process_f32(&self, num_samples: usize, input: &[f32], output: &mut [f32]) {
        let process: ProcessF32Fn = unsafe { std::mem::transmute(self.plugin.process_f32_fn_ptr) };
        unsafe { process(self.ptr, num_samples, input.as_ptr(), output.as_mut_ptr()) };
    }
}

impl Drop for AudioFilterSession {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            self.destroy();
        }
    }
}

pub struct AudioFilterAudioStream {
    inner: NativeAudioStream,
    session: AudioFilterSession,
    buffer: Vec<i16>,
    sample_rate: u32,
    num_channels: u32,
    frame_size: usize,
}

impl AudioFilterAudioStream {
    pub fn new(
        inner: NativeAudioStream,
        session: AudioFilterSession,
        duration: Duration,
        sample_rate: u32,
        num_channels: u32,
    ) -> Self {
        let frame_size =
            ((sample_rate as f64) * duration.as_secs_f64() * num_channels as f64) as usize;
        Self {
            inner,
            session,
            buffer: Vec::with_capacity(frame_size),
            sample_rate,
            num_channels,
            frame_size,
        }
    }
}

impl Stream for AudioFilterAudioStream {
    type Item = AudioFrame<'static>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        while let Poll::Ready(frame) = Pin::new(&mut this.inner).poll_next(cx) {
            let Some(frame) = frame else {
                return Poll::Ready(None);
            };
            this.buffer.extend_from_slice(&frame.data);

            if this.buffer.len() >= this.frame_size {
                let data = this.buffer.drain(..this.frame_size).collect::<Vec<_>>();
                let mut out: Vec<i16> = Vec::with_capacity(this.frame_size);

                this.session.process_i16(this.frame_size, &data, &mut out);

                return Poll::Ready(Some(AudioFrame {
                    data: out.into(),
                    sample_rate: this.sample_rate,
                    num_channels: this.num_channels,
                    samples_per_channel: (this.frame_size / this.num_channels as usize) as u32,
                }));
            }
        }

        Poll::Pending
    }
}

// The function pointers in this struct are initialized only once during construction
// and remain read-only throughout the lifetime of the struct, ensuring thread safety.
unsafe impl Send for AudioFilterPlugin {}
unsafe impl Sync for AudioFilterPlugin {}
unsafe impl Send for AudioFilterSession {}
unsafe impl Sync for AudioFilterSession {}
