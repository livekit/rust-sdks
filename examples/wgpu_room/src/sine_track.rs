use livekit::options::TrackPublishOptions;
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::webrtc::audio_source::RtcAudioSource;
use livekit::webrtc::prelude::AudioSourceOptions;
use livekit::{prelude::*, webrtc::audio_source::native::NativeAudioSource};
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct SineParameters {
    pub sample_rate: u32,
    pub freq: f64,
    pub amplitude: f64,
    pub num_channels: u32,
}

impl Default for SineParameters {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            freq: 440.0,
            amplitude: 1.0,
            num_channels: 2,
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
    params: SineParameters,
    room: Arc<Room>,
    handle: Option<TrackHandle>,
}

impl SineTrack {
    pub fn new(room: Arc<Room>, params: SineParameters) -> Self {
        Self {
            rtc_source: NativeAudioSource::new(
                AudioSourceOptions::default(),
                params.sample_rate,
                params.num_channels,
            ),
            params,
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
            RtcAudioSource::Native(self.rtc_source.clone()),
        );

        let task = tokio::spawn(Self::track_task(
            close_rx,
            self.rtc_source.clone(),
            self.params.clone(),
        ));

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
                .unpublish_track(&handle.track.sid())
                .await?;
        }

        Ok(())
    }

    async fn track_task(
        mut close_rx: oneshot::Receiver<()>,
        rtc_source: NativeAudioSource,
        params: SineParameters,
    ) {
        let num_channels = params.num_channels as usize;
        let samples_count = (params.sample_rate / 150) as usize * num_channels;
        let mut samples_10ms = vec![0; samples_count];
        let mut phase = 0;
        loop {
            if close_rx.try_recv().is_ok() {
                break;
            }

            for i in (0..samples_count).step_by(num_channels) {
                let val = params.amplitude
                    * f64::sin(
                        std::f64::consts::PI
                            * 2.0
                            * params.freq
                            * (phase as f64 / params.sample_rate as f64),
                    );

                phase += 1;

                for c in 0..num_channels {
                    // WebRTC uses 16-bit signed PCM
                    samples_10ms[i + c] = (val * 32768.0) as i16;
                }
            }

            rtc_source
                .capture_frame(&AudioFrame {
                    data: samples_10ms.as_slice().into(),
                    sample_rate: params.sample_rate,
                    num_channels: params.num_channels,
                    samples_per_channel: samples_count as u32 / params.num_channels,
                })
                .await
                .unwrap();
        }
    }
}
