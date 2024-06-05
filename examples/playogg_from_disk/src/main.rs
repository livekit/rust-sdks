use livekit::{
    options::TrackPublishOptions,
    track::{LocalAudioTrack, LocalTrack, TrackSource},
    webrtc::{
        audio_source::native::NativeAudioSource,
        prelude::{AudioFrame, AudioSourceOptions, RtcAudioSource},
    },
    Room, RoomOptions,
};
use std::{env, sync::Arc, time::Duration, error::Error};
use std::fs::File;
use std::io::{BufReader};
use webrtc_media::io::ogg_reader::{OggReader};
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;


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

    let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default())
        .await
        .unwrap();
    let room = Arc::new(room);
    log::info!("Connected to room: {} - {}", room.name(), room.sid());

    let source = NativeAudioSource::new(
        AudioSourceOptions::default(),
        header.sample_rate,
        header.channels as u32,
    );

    let mut audio_opts = AudioSourceOptions::default();
    audio_opts.pre_encoded = true;

    let native_source = RtcAudioSource::Native(source.clone());
    native_source.set_audio_options(audio_opts);

    let track = LocalAudioTrack::create_audio_track("file", native_source);

    room.local_participant()
        .publish_track(
            LocalTrack::Audio(track),
            TrackPublishOptions {
                source: TrackSource::Microphone,
                ..Default::default()
            },
        ).await?;


    tokio::spawn({
        let room = room.clone();
        async move {

            log::info!("sample_rate: {}", header.sample_rate);
            log::info!("num_channels: {}", header.channels);

            while let Ok((page_data, page_header)) = reader.parse_next_page() {
                let frame_size = page_data.len() / 2;

                let mut audio_frame = AudioFrame {
                    data: vec![0i16; 0].into(),
                    sample_rate: header.sample_rate,
                    num_channels: header.channels as u32,
                    samples_per_channel: (frame_size / header.channels as usize) as u32
                };

                let mut rdr = Cursor::new(page_data);
                while let Ok(d) = rdr.read_i16::<LittleEndian>() {
                    audio_frame.data.to_mut().push(d);
                }

                log::info!("sample_rate: {}, num_channels: {}, frame_size: {}, audio_frame.data.len: {}, samples_per_channel: {}",
                audio_frame.sample_rate, audio_frame.num_channels, frame_size, audio_frame.data.len(), audio_frame.samples_per_channel);

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
