use livekit::webrtc::native::apm::AudioProcessingModule;
use log::warn;
use std::sync::Mutex;

const APM_STREAM_DELAY_MS: i32 = 50;

pub struct SharedAudioProcessing {
    apm: Mutex<AudioProcessingModule>,
    sample_rate: u32,
    num_channels: i32,
}

impl SharedAudioProcessing {
    pub fn new(sample_rate: u32, num_channels: i32) -> Self {
        let mut apm = AudioProcessingModule::new(
            true,  // echo cancellation
            false, // AGC disabled by request
            true,  // high-pass filter
            true,  // noise suppression
        );
        if let Err(err) = apm.set_stream_delay_ms(APM_STREAM_DELAY_MS) {
            warn!("APM set_stream_delay_ms failed: {err}");
        }

        Self { apm: Mutex::new(apm), sample_rate, num_channels }
    }

    pub fn process_capture(&self, data: &mut [i16]) {
        if data.is_empty() {
            return;
        }

        if let Err(err) = self.apm.lock().unwrap().process_stream(
            data,
            self.sample_rate as i32,
            self.num_channels,
        ) {
            warn!("APM process_stream failed: {err}");
        }
    }

    pub fn process_render(&self, data: &mut [i16]) {
        if data.is_empty() {
            return;
        }

        if let Err(err) = self.apm.lock().unwrap().process_reverse_stream(
            data,
            self.sample_rate as i32,
            self.num_channels,
        ) {
            warn!("APM process_reverse_stream failed: {err}");
        }
    }
}
