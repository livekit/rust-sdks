use livekit::options::{AudioCaptureOptions, TrackPublishOptions};
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::{prelude::*, webrtc::audio_source::native::NativeAudioSource};
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
    close_tx: oneshot::Sender<()>,
    track: LocalAudioTrack,
    task: JoinHandle<()>,
}

pub struct SineTrack {
    rtc_source: NativeAudioSource,
    room: Arc<Room>,
    handle: Option<TrackHandle>,
}

impl SineTrack {
    pub fn new(room: Arc<Room>) -> Self {
        Self {
            rtc_source: NativeAudioSource::default(),
            room,
            handle: None,
        }
    }

    pub fn is_published(&self) -> bool {
        self.handle.is_some()
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

        let task = tokio::spawn(Self::track_task(close_rx, self.rtc_source.clone()));

        self.room
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
            close_tx,
            track,
            task,
        };

        self.handle = Some(handle);
        Ok(())
    }

    pub async fn unpublish(&mut self) -> Result<(), RoomError> {
        if let Some(handle) = self.handle.take() {
            handle.close_tx.send(()).ok();
            handle.task.await.ok();
            self.room
                .local_participant()
                .unpublish_track(handle.track.sid(), true)
                .await?;
        }

        Ok(())
    }

    async fn track_task(_close_rx: oneshot::Receiver<()>, rtc_source: NativeAudioSource) {
        let mut data = FrameData::default();
        let mut interval = tokio::time::interval(Duration::from_millis(10));
        let mut samples_10ms = Vec::<i16>::new();

        loop {
            const NUM_CHANNELS: usize = 2;

            interval.tick().await;

            let samples_count_10ms = (data.sample_rate / 100) as usize * NUM_CHANNELS;

            if samples_10ms.capacity() != samples_count_10ms {
                samples_10ms.resize(samples_count_10ms, 0i16);
            }

            for i in (0..samples_count_10ms).step_by(NUM_CHANNELS) {
                let val = data.amplitude
                    * f64::sin(
                        std::f64::consts::PI
                            * 2.0
                            * data.freq
                            * (data.phase as f64 / data.sample_rate as f64),
                    );

                data.phase += 1;

                for c in 0..NUM_CHANNELS {
                    // WebRTC uses 16-bit signed PCM
                    samples_10ms[i + c] = (val * 32768.0) as i16;
                }
            }

            rtc_source.capture_frame(&AudioFrame {
                data: samples_10ms.clone(),
                sample_rate: data.sample_rate as u32,
                num_channels: NUM_CHANNELS as u32,
                samples_per_channel: samples_count_10ms as u32,
            });
        }
    }
}
