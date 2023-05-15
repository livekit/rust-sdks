use livekit::options::{AudioCaptureOptions, TrackPublishOptions};
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::{prelude::*, webrtc::audio_source::native::NativeAudioSource};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

#[derive(Clone)]
struct FrameData {
    pub sample_rate: u32,
    pub freq: f64,
    pub amplitude: f64,
    pub phase: u64,
}

impl Default for FrameData {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            freq: 440.0,
            amplitude: 1.0,
            phase: 0,
        }
    }
}

struct TrackHandle {
    frame_data: Arc<Mutex<FrameData>>,
    close_tx: oneshot::Sender<()>,
    track: LocalAudioTrack,
    task: JoinHandle<()>,
}

pub struct SineTrack {
    rtc_source: NativeAudioSource,
    session: RoomSession,
    handle: Option<TrackHandle>,
}

impl SineTrack {
    pub fn new(session: RoomSession) -> Self {
        Self {
            rtc_source: NativeAudioSource::default(),
            session,
            handle: None,
        }
    }

    pub async fn publish(&mut self) -> Result<(), RoomError> {
        let (close_tx, close_rx) = oneshot::channel();
        let track = LocalAudioTrack::create_audio_track(
            "sine_wave",
            AudioCaptureOptions {
                auto_gain_control: false,
                echo_cancellation: false,
                noise_suppression: false,
            },
            self.rtc_source.clone(),
        );

        let data = Arc::new(Mutex::new(FrameData::default()));
        let task = tokio::spawn(Self::track_task(
            close_rx,
            self.rtc_source.clone(),
            data.clone(),
        ));

        self.session
            .local_participant()
            .publish_track(
                LocalTrack::Audio(track.clone()),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await?;

        let handle = TrackHandle {
            frame_data: data,
            close_tx,
            track,
            task,
        };

        self.handle = Some(handle);
        Ok(())
    }

    async fn track_task(
        _close_rx: oneshot::Receiver<()>,
        rtc_source: NativeAudioSource,
        frame_options: Arc<Mutex<FrameData>>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_millis(20));
        let mut samples_10ms = Vec::<i16>::new();

        loop {
            interval.tick().await;

            let mut data = frame_options.lock();
            let samples_count_10ms = (data.sample_rate / 50) as usize;

            if samples_10ms.capacity() != samples_count_10ms {
                samples_10ms.resize(samples_count_10ms, 0i16);
            }

            for i in 0..samples_count_10ms {
                let val = data.amplitude
                    * f64::sin(
                        std::f64::consts::PI
                            * 2.0
                            * data.freq
                            * (data.phase as f64 / data.sample_rate as f64),
                    );

                data.phase += 1;
                // WebRTC uses 16-bit signed PCM
                samples_10ms[i] = (val * 32767.0) as i16;
            }

            rtc_source.capture_frame(&AudioFrame {
                data: samples_10ms.clone(),
                sample_rate: data.sample_rate as u32,
                num_channels: 1,
                samples_per_channel: samples_count_10ms as u32,
            });
        }
    }
}
