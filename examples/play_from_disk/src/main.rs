use livekit::{
    options::TrackPublishOptions,
    track::{LocalAudioTrack, LocalTrack, TrackSource},
    webrtc::{
        audio_source::native::NativeAudioSource,
        prelude::{AudioFrame, AudioSourceOptions, RtcAudioSource},
    },
    Room, RoomOptions,
};
use std::{env, io::SeekFrom, mem::size_of, sync::Arc, time::Duration};
use std::{error::Error, io};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, BufReader};

#[derive(Debug, Error)]
pub enum WavError {
    #[error("Invalid header: {0}")]
    InvalidHeader(&'static str),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

pub struct WavReader<R: AsyncRead + AsyncSeek + Unpin> {
    reader: R,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct WavHeader {
    file_size: u32,
    data_size: u32,
    format: String,
    format_length: u32,
    format_type: u16,
    num_channels: u16,
    sample_rate: u32,
    byte_rate: u32,
    block_align: u16,
    bits_per_sample: u16,
}

impl<R: AsyncRead + AsyncSeek + Unpin> WavReader<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    pub async fn read_header(&mut self) -> Result<WavHeader, WavError> {
        let mut header = [0u8; 4];
        let mut format = [0u8; 4];
        let mut chunk_marker = [0u8; 4];
        let mut data_chunk = [0u8; 4];

        self.reader.read_exact(&mut header).await?;

        if &header != b"RIFF" {
            return Err(WavError::InvalidHeader("Invalid RIFF header"));
        }

        let file_size = self.reader.read_u32_le().await?;
        self.reader.read_exact(&mut format).await?;

        if &format != b"WAVE" {
            return Err(WavError::InvalidHeader("Invalid WAVE header"));
        }

        self.reader.read_exact(&mut chunk_marker).await?;

        if &chunk_marker != b"fmt " {
            return Err(WavError::InvalidHeader("Invalid fmt chunk"));
        }

        let format_length = self.reader.read_u32_le().await?;
        let format_type = self.reader.read_u16_le().await?;
        let num_channels = self.reader.read_u16_le().await?;
        let sample_rate = self.reader.read_u32_le().await?;
        let byte_rate = self.reader.read_u32_le().await?;
        let block_align = self.reader.read_u16_le().await?;
        let bits_per_sample = self.reader.read_u16_le().await?;

        let mut data_size = 0;
        loop {
            self.reader.read_exact(&mut data_chunk).await?;
            data_size = self.reader.read_u32_le().await?;

            if &data_chunk == b"data" {
                break;
            } else {
                // skip non data chunks
                self.reader.seek(SeekFrom::Current(data_size.into())).await?;
            }
        }

        if &data_chunk != b"data" {
            return Err(WavError::InvalidHeader("Invalid data chunk"));
        }

        Ok(WavHeader {
            file_size,
            data_size,
            format: String::from_utf8_lossy(&format).to_string(),
            format_length,
            format_type,
            num_channels,
            sample_rate,
            byte_rate,
            block_align,
            bits_per_sample,
        })
    }

    pub async fn read_i16(&mut self) -> Result<i16, WavError> {
        let i = self.reader.read_i16_le().await?;
        Ok(i)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let file = tokio::fs::File::open("change-sophie.wav").await?;
    let mut reader = WavReader::new(BufReader::new(file));
    let header = reader.read_header().await?;
    log::debug!("{:?}", header);

    if header.bits_per_sample != 16 {
        return Err("only 16-bit samples supported for this demo".into());
    }

    let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default()).await.unwrap();
    let room = Arc::new(room);
    log::info!("Connected to room: {} - {}", room.name(), room.sid().await);

    let source = NativeAudioSource::new(
        AudioSourceOptions::default(),
        header.sample_rate,
        header.num_channels as u32,
        1000,
    );

    let track = LocalAudioTrack::create_audio_track("file", RtcAudioSource::Native(source.clone()));

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
            const FRAME_DURATION: Duration = Duration::from_millis(1000); // Write 1s of audio at a time

            let max_samples = header.data_size as usize / size_of::<i16>();
            let ms = FRAME_DURATION.as_millis() as u32;
            let num_samples = (header.sample_rate / 1000 * ms) as usize;

            log::info!("sample_rate: {}", header.sample_rate);
            log::info!("num_channels: {}", header.num_channels);
            log::info!("max samples: {}", max_samples);
            log::info!("chunk size: {}ms - {} samples", ms, num_samples);

            let mut written_samples = 0;
            while written_samples < max_samples {
                let available_samples = max_samples - written_samples;
                let frame_size = num_samples.min(available_samples);

                let mut audio_frame = AudioFrame {
                    data: vec![0i16; frame_size].into(),
                    num_channels: header.num_channels as u32,
                    sample_rate: header.sample_rate,
                    samples_per_channel: (frame_size / header.num_channels as usize) as u32,
                };

                for i in 0..frame_size {
                    let sample = reader.read_i16().await.unwrap();
                    audio_frame.data.to_mut()[i] = sample;
                }

                source.capture_frame(&audio_frame).await.unwrap();
                written_samples += frame_size;
            }

            room.close().await.unwrap();
        }
    });

    while let Some(msg) = rx.recv().await {
        log::info!("Event: {:?}", msg);
    }

    Ok(())
}
