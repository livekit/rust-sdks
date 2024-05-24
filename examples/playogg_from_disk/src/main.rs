use livekit::{
    options::TrackPublishOptions,
    track::{LocalAudioTrack, LocalTrack, TrackSource},
    webrtc::{
        audio_source::native::EncodedAudioSource,
        audio_source::native::NativeAudioSource,
        prelude::{AudioFrame_u8, AudioSourceOptions, RtcAudioSource},
    },
    Room, RoomOptions,
};
use std::fs::File;
use std::io::BufReader;
use std::{env, error::Error, sync::Arc, time::Duration};
use webrtc_media::io::ogg_reader::OggReader;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let args: Vec<String> = env::args().collect();
    log::debug!("{:?}", args);

    let file = File::open(&args[1])?;
    let (mut reader, header) = match OggReader::new(BufReader::new(file), false) {
        Ok(ogg) => ogg,
        Err(err) => {
            return Err(err.into());
        }
    };
    log::info!("OggHeader:\nchannel_map: {}, channels: {}, output_gain: {}, pre_skip: {}, sample_rate: {}, version: {}",
        header.channel_map,
        header.channels,
        header.output_gain,
        header.pre_skip,
        header.sample_rate,
        header.version,
    );

    let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default()).await.unwrap();
    let room = Arc::new(room);
    log::info!("Connected to room: {} - {}", room.name(), room.sid());

    let source = EncodedAudioSource::new(
        AudioSourceOptions::default(),
        header.sample_rate,
        header.channels as u32,
    );

    let mut audio_opts = AudioSourceOptions::default();
    audio_opts.pre_encoded = true;

    let encoded_source = RtcAudioSource::Encoded(source.clone());
    encoded_source.set_audio_options(audio_opts);

    let track = LocalAudioTrack::create_encoded_audio_track("file", encoded_source);

    room.local_participant()
        .publish_track(
            LocalTrack::Audio(track),
            TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() },
        )
        .await?;

    // Play the wav file and disconnect
    tokio::spawn({
        let room = room.clone();
        async move {
            log::info!("sample_rate: {}", header.sample_rate);
            log::info!("num_channels: {}", header.channels);

            while let Ok((page_data, page_header)) = reader.parse_next_page() {
                let frame_size = page_data.len();
                let mut audio_frame = AudioFrame_u8 {
                    data: page_data.freeze().into(),
                    num_channels: header.channels as u32,
                    sample_rate: header.sample_rate,
                    samples_per_channel: (frame_size / header.channels as usize) as u32,
                };

                source.capture_frame(&audio_frame).await.unwrap();
            }

            room.close().await.unwrap();
        }
    });

    while let Some(msg) = rx.recv().await {
        log::info!("Event: {:?}", msg);
    }

    Ok(())
}
