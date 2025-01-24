use std::{
    ffi::{c_char, c_void, CString},
    sync::Arc,
};

use libloading::{Library, Symbol};
use parking_lot::{Mutex, RwLock};

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("dylib error: {0}")]
    Library(#[from] libloading::Error),
}

type CreateFn = unsafe extern "C" fn(sampling_rate: u32, options: *const c_char) -> *mut c_void;
type DestroyFn = unsafe extern "C" fn(*const c_void);
type ProcessI16Fn = unsafe extern "C" fn(*const c_void, usize, *const i16, *mut i16);
type ProcessF32Fn = unsafe extern "C" fn(*const c_void, usize, *const f32, *mut f32);

pub struct AudioFilterPlugin {
    lib: Library,
    create_fn_ptr: *const c_void,
    destroy_fn_ptr: *const c_void,
    process_i16_fn_ptr: *const c_void,
    process_f32_fn_ptr: *const c_void,
}

impl AudioFilterPlugin {
    pub fn new<P: AsRef<str>>(path: P) -> Result<Arc<Self>, PluginError> {
        let lib = unsafe { Library::new(path.as_ref()) }?;

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

        Ok(Arc::new(Self {
            lib,
            create_fn_ptr,
            destroy_fn_ptr,
            process_i16_fn_ptr,
            process_f32_fn_ptr,
        }))
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

// The function pointers in this struct are initialized only once during construction
// and remain read-only throughout the lifetime of the struct, ensuring thread safety.
unsafe impl Send for AudioFilterPlugin {}
unsafe impl Sync for AudioFilterPlugin {}
unsafe impl Send for AudioFilterSession {}
unsafe impl Sync for AudioFilterSession {}
