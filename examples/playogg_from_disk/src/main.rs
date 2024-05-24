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


const OGG_PAGE_DURATION: Duration = Duration::from_millis(20);

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
            // It is important to use a time.Ticker instead of time.Sleep because
            // * avoids accumulating skew, just calling time.Sleep didn't compensate for the time spent parsing the data
            // * works around latency issues with Sleep
            let mut ticker = tokio::time::interval(OGG_PAGE_DURATION);

            // Keep track of last granule, the difference is the amount of samples in the buffer
            let mut last_granule: u64 = 0;

            while let Ok((page_data, page_header)) = reader.parse_next_page() {
                // The amount of samples is the difference between the last and current timestamp
                let sample_count = page_header.granule_position - last_granule;
                last_granule = page_header.granule_position;

                let mut audio_frame = AudioFrame {
                    data: vec![0i16; 0].into(),
                    sample_rate: header.sample_rate,
                    num_channels: header.channels as u32,
                    samples_per_channel: (sample_count / header.channels as u64) as u32
                };

                let mut rdr = Cursor::new(page_data.freeze());
                while let Ok(d) = rdr.read_i16::<LittleEndian>() {
                    audio_frame.data.to_mut().push(d);
                }

                source.capture_frame(&audio_frame).await.unwrap();

                let _ = ticker.tick().await;
            }

            room.close().await.unwrap();
        }
    });

    while let Some(msg) = rx.recv().await {
        log::info!("Event: {:?}", msg);
    }

    Ok(())
}
