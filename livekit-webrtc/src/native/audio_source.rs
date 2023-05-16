use crate::audio_frame::AudioFrame;
use cxx::SharedPtr;
use std::sync::{Arc, Mutex};
use webrtc_sys::media_stream as sys_ms;

#[derive(Clone)]
pub struct NativeAudioSource {
    sys_handle: SharedPtr<sys_ms::ffi::AudioTrackSource>,
    inner: Arc<Mutex<AudioSourceInner>>,
}

#[derive(Default)]
struct AudioSourceInner {
    buf: Vec<i16>,
    offset: usize,
    sample_rate: u32,
    num_channels: u32,
}

impl Default for NativeAudioSource {
    fn default() -> Self {
        Self {
            sys_handle: sys_ms::ffi::new_audio_track_source(),
            inner: Default::default(),
        }
    }
}

impl NativeAudioSource {
    pub fn sys_handle(&self) -> SharedPtr<sys_ms::ffi::AudioTrackSource> {
        self.sys_handle.clone()
    }

    pub fn capture_frame(&self, frame: &AudioFrame) {
        let mut inner = self.inner.lock().unwrap();
        let samples_10ms = (frame.sample_rate / 100 * frame.num_channels) as usize;
        if inner.sample_rate != frame.sample_rate || inner.num_channels != frame.num_channels {
            inner.buf.resize(samples_10ms as usize, 0);
            inner.offset = 0;
        }

        // Split the frame into 10ms chunks
        let mut i = 0;
        loop {
            let buf_offset = inner.offset;
            let remaining_data = frame.data.len() - i; // Remaining data to read inside the frame
            let needed_data = samples_10ms - buf_offset; // Needed data of "frame.data" to make a complete 10ms from inner.buf
            if remaining_data < needed_data {
                if remaining_data > 0 {
                    // Not enough data to make a complete 10ms frame, store the remaining data inside inner.buf
                    // It'll be used on the next capture.
                    inner.buf[buf_offset..buf_offset + remaining_data]
                        .copy_from_slice(&frame.data[i..]);
                    inner.offset += remaining_data;
                }

                break;
            }

            let data = if inner.offset != 0 {
                // Use the data from the previous capture
                let data = &mut inner.buf[buf_offset..];
                data.copy_from_slice(&frame.data[i..i + needed_data]);
                inner.offset = 0;
                &inner.buf
            } else {
                &frame.data[i..i + samples_10ms]
            };

            unsafe {
                self.sys_handle.on_captured_frame(
                    data.as_ptr(),
                    frame.sample_rate as i32,
                    frame.num_channels as usize,
                    samples_10ms,
                )
            }

            i += needed_data;
        }
    }
}
