// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Encodes an I420 file to H.264 with the V4L2 encoder, for bringing up a board
//! without WebRTC. Halfway through, it halves the bitrate and forces a keyframe.
//!
//! ```text
//! cargo run -p livekit-hwcodec --example encode_file -- in.i420 1280 720 out.h264
//! ```

#[cfg(not(all(target_os = "linux", target_pointer_width = "64")))]
fn main() {
    eprintln!("the V4L2 encoder needs 64-bit Linux");
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        fs::File,
        io::{Read, Write},
        sync::{mpsc, Arc, Mutex},
        thread,
        time::{Duration, Instant},
    };

    use livekit_hwcodec::{
        Codec, EncodeStatus, EncoderConfig, Frame, FrameMeta, Plane, Sink, V4l2EncoderFactory, I420,
    };

    const FRAMERATE: u32 = 30;
    const BITRATE_BPS: u32 = 2_000_000;

    #[derive(Clone)]
    struct RawFrame {
        width: u32,
        height: u32,
        data: Arc<[u8]>,
    }

    impl Frame for RawFrame {
        fn with_i420<R>(&self, f: impl FnOnce(I420<'_>) -> R) -> Option<R> {
            let (width, height) = (self.width as usize, self.height as usize);
            let (chroma_width, chroma_height) = (width.div_ceil(2), height.div_ceil(2));
            let (y, chroma) = self.data.split_at_checked(width * height)?;
            let (u, v) = chroma.split_at_checked(chroma_width * chroma_height)?;
            Some(f(I420 {
                width: self.width,
                height: self.height,
                y: Plane { data: y, stride: width },
                u: Plane { data: u, stride: chroma_width },
                v: Plane { data: v, stride: chroma_width },
            }))
        }
    }

    struct FileSink {
        file: Mutex<File>,
        started: Instant,
        done: mpsc::Sender<()>,
    }

    impl Sink for FileSink {
        fn deliver(&self, data: &[u8], meta: &FrameMeta, keyframe: bool) {
            let latency = self.started.elapsed().as_millis() as i64 - meta.capture_time_ms;
            let kind = if keyframe { "key" } else { "delta" };
            println!(
                "frame {:>5}: {:>7} bytes, {kind}, {latency} ms",
                meta.rtp_timestamp,
                data.len()
            );
            if let Err(error) = self.file.lock().unwrap().write_all(data) {
                eprintln!("write failed: {error}");
            }
            let _ = self.done.send(());
        }

        fn dropped(&self, meta: &FrameMeta) {
            println!("frame {:>5}: dropped by the encoder", meta.rtp_timestamp);
            let _ = self.done.send(());
        }
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    let args: Vec<String> = std::env::args().collect();
    let [_, input, width, height, output] = &args[..] else {
        return Err("usage: encode_file <in.i420> <width> <height> <out.h264>".into());
    };
    let (width, height): (u32, u32) = (width.parse()?, height.parse()?);
    let frame_size =
        (width * height) as usize + 2 * (width.div_ceil(2) * height.div_ceil(2)) as usize;
    let mut raw = Vec::new();
    File::open(input)?.read_to_end(&mut raw)?;
    let frames: Vec<RawFrame> = raw
        .chunks_exact(frame_size)
        .map(|data| RawFrame { width, height, data: data.into() })
        .collect();

    let factory = V4l2EncoderFactory::probe();
    let mut encoder =
        factory.create::<RawFrame>(Codec::H264).ok_or("no V4L2 H.264 encoder found")?;
    let started = Instant::now();
    let (done, finished) = mpsc::channel();
    let sink = FileSink { file: Mutex::new(File::create(output)?), started, done };
    let config = EncoderConfig { width, height, bitrate_bps: BITRATE_BPS, framerate: FRAMERATE };
    if !encoder.init(config, sink) {
        return Err("the encoder thread could not be started".into());
    }

    let mut queued = 0;
    for (index, frame) in frames.iter().enumerate() {
        let halfway = index == frames.len() / 2;
        if halfway {
            println!("halving the bitrate and forcing a keyframe");
            encoder.set_rates(BITRATE_BPS / 2, FRAMERATE);
        }
        let meta = FrameMeta {
            rtp_timestamp: index as u32,
            capture_time_ms: started.elapsed().as_millis() as i64,
        };
        match encoder.encode(frame.clone(), meta, halfway) {
            EncodeStatus::Queued => queued += 1,
            EncodeStatus::Dropped => println!("frame {index:>5}: dropped, the encoder is full"),
            EncodeStatus::Failed => return Err("the encoder failed; see the log".into()),
        }
        thread::sleep(Duration::from_secs(1) / FRAMERATE);
    }
    for _ in 0..queued {
        finished.recv_timeout(Duration::from_secs(5)).map_err(|_| "the encoder stalled")?;
    }
    Ok(())
}
