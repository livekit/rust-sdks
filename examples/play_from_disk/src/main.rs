use std::io;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Debug, Error)]
enum WavError {
    #[error("Invalid header: {0}")]
    InvalidHeader(&'static str),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

struct WavReader<R: AsyncRead + Unpin> {
    reader: R,
}

#[derive(Debug)]
struct WavHeader {
    file_size: u32,
    format: String,
    format_length: u32,
    format_type: u16,
    num_channels: u16,
    sample_rate: u32,
    byte_rate: u32,
    block_align: u16,
    bits_per_sample: u16,
}

impl<R: AsyncRead + Unpin> WavReader<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    async fn read_header(&mut self) -> Result<WavHeader, WavError> {
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
        self.reader.read_exact(&mut data_chunk).await?;

        if &data_chunk != b"data" {
            return Err(WavError::InvalidHeader("Invalid data chunk"));
        }

        Ok(WavHeader {
            file_size,
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
}

#[tokio::main]
async fn main() {
    let mut reader = WavReader::new(tokio::fs::File::open("change-sophie.wav").await.unwrap());
    let header = reader.read_header().await.unwrap();
    println!("{:?}", header);
}
