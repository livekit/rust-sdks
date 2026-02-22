use futures::StreamExt;
use lazy_static::lazy_static;
use livekit::{
    options::TrackPublishOptions,
    prelude::*,
    track::{LocalAudioTrack, LocalTrack, RemoteTrack, TrackSource},
    webrtc::{
        audio_frame::AudioFrame,
        audio_source::native::NativeAudioSource,
        audio_stream::native::NativeAudioStream,
        prelude::{AudioSourceOptions, RtcAudioSource},
    },
    Room, RoomOptions,
};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

const SAMPLE_RATE: u32 = 48000;
const NUM_CHANNELS: u32 = 1;
const SAMPLES_PER_10MS: u32 = SAMPLE_RATE / 100; // 480 samples per 10ms frame

struct AudioState {
    // For sending microphone audio to LiveKit
    audio_source: Option<NativeAudioSource>,
    capture_buffer: VecDeque<i16>,

    // For receiving remote audio from LiveKit
    playback_buffer: VecDeque<i16>,

    // Room reference
    room: Option<Arc<Room>>,

    // Track if we're connected
    is_connected: bool,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            audio_source: None,
            capture_buffer: VecDeque::with_capacity(SAMPLE_RATE as usize), // 1 second buffer
            playback_buffer: VecDeque::with_capacity(SAMPLE_RATE as usize),
            room: None,
            is_connected: false,
        }
    }
}

struct App {
    async_runtime: tokio::runtime::Runtime,
    audio_state: Arc<Mutex<AudioState>>,
}

impl Default for App {
    fn default() -> Self {
        App {
            async_runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
            audio_state: Arc::new(Mutex::new(AudioState::default())),
        }
    }
}

lazy_static! {
    static ref APP: App = App::default();
}

/// Connect to a LiveKit room and set up audio handling
pub fn livekit_connect(url: String, token: String) {
    log::info!("Connecting to {} with token {}", url, token);

    let audio_state = APP.audio_state.clone();

    APP.async_runtime.spawn(async move {
        // Create audio source for microphone capture
        let audio_source = NativeAudioSource::new(
            AudioSourceOptions {
                echo_cancellation: false,
                noise_suppression: false,
                auto_gain_control: false,
            },
            SAMPLE_RATE,
            NUM_CHANNELS,
            100, // 100ms buffer
        );

        // Store audio source
        {
            let mut state = audio_state.lock();
            state.audio_source = Some(audio_source.clone());
        }

        // Connect to room
        let mut room_options = RoomOptions::default();
        room_options.auto_subscribe = true;

        let res = Room::connect(&url, &token, room_options).await;

        if let Err(err) = res {
            log::error!("Failed to connect: {}", err);
            return;
        }

        let (room, mut events) = res.unwrap();
        let room = Arc::new(room);

        log::info!("Connected to room {}", String::from(room.sid().await));

        // Store room reference
        {
            let mut state = audio_state.lock();
            state.room = Some(room.clone());
            state.is_connected = true;
        }

        // Create and publish local audio track
        let track =
            LocalAudioTrack::create_audio_track("microphone", RtcAudioSource::Native(audio_source));

        if let Err(e) = room
            .local_participant()
            .publish_track(
                LocalTrack::Audio(track),
                TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() },
            )
            .await
        {
            log::error!("Failed to publish audio track: {}", e);
        } else {
            log::info!("Published local audio track");
        }

        // Handle room events
        while let Some(event) = events.recv().await {
            match event {
                RoomEvent::TrackSubscribed { track, publication, participant } => {
                    log::info!(
                        "Track subscribed from {}: {} ({:?})",
                        participant.identity(),
                        publication.name(),
                        track.kind()
                    );

                    if let RemoteTrack::Audio(audio_track) = track {
                        let audio_state_clone = audio_state.clone();
                        let participant_id = participant.identity().to_string();

                        // Spawn task to handle this audio stream
                        tokio::spawn(async move {
                            handle_remote_audio_stream(
                                audio_track,
                                audio_state_clone,
                                participant_id,
                            )
                            .await;
                        });
                    }
                }
                RoomEvent::TrackUnsubscribed { track, participant, .. } => {
                    log::info!(
                        "Track unsubscribed from {}: {:?}",
                        participant.identity(),
                        track.kind()
                    );
                }
                RoomEvent::ParticipantConnected(participant) => {
                    log::info!(
                        "Participant connected: {} ({})",
                        participant.identity(),
                        participant.name()
                    );
                }
                RoomEvent::ParticipantDisconnected(participant) => {
                    log::info!("Participant disconnected: {}", participant.identity());
                }
                RoomEvent::Disconnected { reason } => {
                    log::info!("Disconnected from room: {:?}", reason);
                    let mut state = audio_state.lock();
                    state.is_connected = false;
                    state.room = None;
                    break;
                }
                _ => {
                    log::debug!("Room event: {:?}", event);
                }
            }
        }
    });
}

/// Handle incoming audio from a remote participant
async fn handle_remote_audio_stream(
    audio_track: RemoteAudioTrack,
    audio_state: Arc<Mutex<AudioState>>,
    participant_id: String,
) {
    log::info!(
        "Starting audio stream for participant: {}, track sid: {:?}",
        participant_id,
        audio_track.sid()
    );

    let mut audio_stream =
        NativeAudioStream::new(audio_track.rtc_track(), SAMPLE_RATE as i32, NUM_CHANNELS as i32);

    let mut frame_count: u64 = 0;
    let mut total_samples: u64 = 0;

    while let Some(frame) = audio_stream.next().await {
        let samples: &[i16] = frame.data.as_ref();
        frame_count += 1;
        total_samples += samples.len() as u64;

        // Log every 100 frames (~1 second)
        if frame_count % 100 == 0 {
            log::info!(
                "Audio stream [{}]: received frame #{}, {} samples this frame, {} total samples, sample_rate={}, channels={}",
                participant_id,
                frame_count,
                samples.len(),
                total_samples,
                frame.sample_rate,
                frame.num_channels
            );
        }

        // Add samples to playback buffer
        let mut state = audio_state.lock();

        // Limit buffer size to prevent memory growth (keep ~500ms max)
        let max_buffer_size = (SAMPLE_RATE / 2) as usize;
        while state.playback_buffer.len() + samples.len() > max_buffer_size {
            state.playback_buffer.pop_front();
        }

        let buffer_size_before = state.playback_buffer.len();
        for &sample in samples {
            state.playback_buffer.push_back(sample);
        }

        // Log buffer state periodically
        if frame_count % 100 == 0 {
            log::info!(
                "Playback buffer: {} -> {} samples",
                buffer_size_before,
                state.playback_buffer.len()
            );
        }
    }

    log::info!(
        "Audio stream ended for participant: {}, total frames: {}, total samples: {}",
        participant_id,
        frame_count,
        total_samples
    );
}

/// Push captured microphone audio to LiveKit (called from Kotlin)
/// Returns the number of samples consumed
pub fn push_audio_capture(samples: &[i16]) -> usize {
    let mut state = APP.audio_state.lock();

    // Add to capture buffer
    for &sample in samples {
        state.capture_buffer.push_back(sample);
    }

    // Process complete 10ms frames
    let mut frames_sent = 0;
    while state.capture_buffer.len() >= SAMPLES_PER_10MS as usize {
        let mut frame_data: Vec<i16> = Vec::with_capacity(SAMPLES_PER_10MS as usize);
        for _ in 0..SAMPLES_PER_10MS {
            if let Some(sample) = state.capture_buffer.pop_front() {
                frame_data.push(sample);
            }
        }

        if let Some(ref audio_source) = state.audio_source {
            let audio_frame = AudioFrame {
                data: frame_data.into(),
                sample_rate: SAMPLE_RATE,
                num_channels: NUM_CHANNELS,
                samples_per_channel: SAMPLES_PER_10MS,
            };

            // Use blocking capture since we're called from a sync context
            let source = audio_source.clone();
            drop(state); // Release lock before async operation

            APP.async_runtime.spawn(async move {
                if let Err(e) = source.capture_frame(&audio_frame).await {
                    log::error!("Failed to capture audio frame: {}", e);
                }
            });

            state = APP.audio_state.lock();
            frames_sent += 1;
        }
    }

    if frames_sent > 0 {
        log::trace!("Sent {} audio frames to LiveKit", frames_sent);
    }

    samples.len()
}

/// Pull playback audio from LiveKit (called from Kotlin)
/// Returns the number of samples written to the buffer
pub fn pull_audio_playback(buffer: &mut [i16]) -> usize {
    static PULL_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let mut state = APP.audio_state.lock();

    let available = state.playback_buffer.len();
    let to_copy = available.min(buffer.len());

    for i in 0..to_copy {
        if let Some(sample) = state.playback_buffer.pop_front() {
            buffer[i] = sample;
        }
    }

    // Fill remaining with silence
    for i in to_copy..buffer.len() {
        buffer[i] = 0;
    }

    // Log periodically
    let count = PULL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if count % 100 == 0 {
        log::debug!(
            "pull_audio_playback #{}: requested={}, available={}, copied={}",
            count,
            buffer.len(),
            available,
            to_copy
        );
    }

    to_copy
}

/// Check if we're connected to a room
pub fn is_connected() -> bool {
    APP.audio_state.lock().is_connected
}

/// Get the number of samples available for playback
pub fn get_playback_buffer_size() -> usize {
    APP.audio_state.lock().playback_buffer.len()
}

/// Disconnect from the room
pub fn disconnect() {
    let room = {
        let mut state = APP.audio_state.lock();
        state.is_connected = false;
        state.room.take()
    };

    if let Some(room) = room {
        APP.async_runtime.spawn(async move {
            if let Err(e) = room.close().await {
                log::error!("Error closing room: {}", e);
            }
            log::info!("Disconnected from room");
        });
    }
}

// ============================================================================
// iOS Implementation
// ============================================================================

#[cfg(target_os = "ios")]
pub mod ios {
    use std::ffi::{c_char, CStr};

    #[no_mangle]
    pub extern "C" fn livekit_connect(url: *const c_char, token: *const c_char) {
        let (url, token) = unsafe {
            let url = CStr::from_ptr(url).to_str().unwrap().to_owned();
            let token = CStr::from_ptr(token).to_str().unwrap().to_owned();
            (url, token)
        };

        super::livekit_connect(url, token);
    }

    #[no_mangle]
    pub extern "C" fn livekit_push_audio(samples: *const i16, count: usize) -> usize {
        let slice = unsafe { std::slice::from_raw_parts(samples, count) };
        super::push_audio_capture(slice)
    }

    #[no_mangle]
    pub extern "C" fn livekit_pull_audio(buffer: *mut i16, count: usize) -> usize {
        let slice = unsafe { std::slice::from_raw_parts_mut(buffer, count) };
        super::pull_audio_playback(slice)
    }

    #[no_mangle]
    pub extern "C" fn livekit_disconnect() {
        super::disconnect();
    }

    #[no_mangle]
    pub extern "C" fn livekit_is_connected() -> bool {
        super::is_connected()
    }
}

// ============================================================================
// Android Implementation
// ============================================================================

#[cfg(target_os = "android")]
pub mod android {
    use android_logger::Config;
    use jni::{
        objects::{JClass, JShortArray, JString},
        sys::{jboolean, jint, JNI_VERSION_1_6},
        JNIEnv, JavaVM,
    };
    use log::LevelFilter;
    use std::os::raw::c_void;

    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "C" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
        android_logger::init_once(
            Config::default().with_max_level(LevelFilter::Debug).with_tag("livekit-rustexample"),
        );

        log::info!("JNI_OnLoad, initializing LiveKit");
        livekit::webrtc::android::initialize_android(&vm);
        JNI_VERSION_1_6
    }

    /// Connect to a LiveKit room
    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "C" fn Java_io_livekit_rustexample_App_connectNative(
        mut env: JNIEnv,
        _: JClass,
        url: JString,
        token: JString,
    ) {
        let url: String = env.get_string(&url).unwrap().into();
        let token: String = env.get_string(&token).unwrap().into();

        super::livekit_connect(url, token);
    }

    /// Disconnect from the room
    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "C" fn Java_io_livekit_rustexample_App_disconnectNative(_env: JNIEnv, _: JClass) {
        super::disconnect();
    }

    /// Check if connected to a room
    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "C" fn Java_io_livekit_rustexample_App_isConnectedNative(
        _env: JNIEnv,
        _: JClass,
    ) -> jboolean {
        if super::is_connected() {
            1
        } else {
            0
        }
    }

    /// Push captured audio samples to LiveKit
    /// Takes a short array (16-bit PCM samples)
    /// Returns the number of samples consumed
    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "C" fn Java_io_livekit_rustexample_App_pushAudioNative(
        env: JNIEnv,
        _: JClass,
        samples: JShortArray,
    ) -> jint {
        let len = match env.get_array_length(&samples) {
            Ok(l) => l as usize,
            Err(e) => {
                log::error!("Failed to get array length: {}", e);
                return 0;
            }
        };

        if len == 0 {
            return 0;
        }

        let mut buffer: Vec<i16> = vec![0i16; len];
        if let Err(e) = env.get_short_array_region(&samples, 0, &mut buffer) {
            log::error!("Failed to get short array region: {}", e);
            return 0;
        }

        super::push_audio_capture(&buffer) as jint
    }

    /// Pull playback audio from LiveKit
    /// Fills the provided short array with PCM samples
    /// Returns the number of actual samples written (rest is silence)
    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "C" fn Java_io_livekit_rustexample_App_pullAudioNative(
        env: JNIEnv,
        _: JClass,
        buffer: JShortArray,
    ) -> jint {
        let len = match env.get_array_length(&buffer) {
            Ok(l) => l as usize,
            Err(e) => {
                log::error!("Failed to get array length: {}", e);
                return 0;
            }
        };

        if len == 0 {
            return 0;
        }

        let mut rust_buffer: Vec<i16> = vec![0i16; len];
        let samples_written = super::pull_audio_playback(&mut rust_buffer);

        if let Err(e) = env.set_short_array_region(&buffer, 0, &rust_buffer) {
            log::error!("Failed to set short array region: {}", e);
            return 0;
        }

        samples_written as jint
    }

    /// Get the number of samples available in the playback buffer
    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "C" fn Java_io_livekit_rustexample_App_getPlaybackBufferSizeNative(
        _env: JNIEnv,
        _: JClass,
    ) -> jint {
        super::get_playback_buffer_size() as jint
    }
}
