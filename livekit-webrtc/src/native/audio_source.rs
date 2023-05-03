use crate::audio_frame::AudioFrame;
use cxx::SharedPtr;
use webrtc_sys::media_stream as sys_ms;

#[derive(Clone)]
pub struct NativeAudioSource {
    sys_handle: SharedPtr<sys_ms::ffi::AudioTrackSource>,
}

impl Default for NativeAudioSource {
    fn default() -> Self {
        Self {
            sys_handle: sys_ms::ffi::new_audio_track_source(),
        }
    }
}

impl NativeAudioSource {
    pub fn sys_handle(&self) -> SharedPtr<sys_ms::ffi::AudioTrackSource> {
        self.sys_handle.clone()
    }

    pub fn capture_frame(&self, frame: &AudioFrame) {
        // TODO(theomonnom): Should we check for 10ms worth of data here?
        unsafe {
            self.sys_handle.on_captured_frame(
                frame.data.as_ptr(),
                frame.sample_rate as i32,
                frame.num_channels as usize,
                frame.samples_per_channel as usize,
            )
        }
    }
}
