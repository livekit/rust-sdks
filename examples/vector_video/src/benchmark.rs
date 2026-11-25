//! Runs each lossless wire representation against identical visual encoder decisions.
use std::time::{Duration, Instant};

use anyhow::{ensure, Context, Result};

use crate::{
    codec::WireFormat,
    compression::{Compression, PacketEncoder},
    messages::{self, Receiver},
    video::Source,
    FileInput,
};

/// Accumulates byte counts and timing for a single compression method.
struct Trial {
    name: &'static str,
    packer: PacketEncoder,
    decoder: Receiver,
    bytes: u64,
    messages: u64,
    encode: Duration,
    decode: Duration,
    polygons: u64,
    peak_polygons: usize,
}

impl Trial {
    /// Starts a comparison with fresh reference frames.
    fn new(name: &'static str, packer: PacketEncoder) -> Self {
        Self {
            name,
            packer,
            decoder: Receiver::default(),
            bytes: 0,
            messages: 0,
            encode: Duration::ZERO,
            decode: Duration::ZERO,
            polygons: 0,
            peak_polygons: 0,
        }
    }

    /// Fails immediately if a compressed packet changes the original snapshot or exceeds its size.
    fn measure(&mut self, index: u32, raw: &[u8]) -> Result<crate::codec::DecodedFrame> {
        let start = Instant::now();
        let mut snapshot = raw.to_vec();
        self.packer.stabilize_outline_order(&mut snapshot);
        let packed = self.packer.pack(snapshot.clone())?;
        self.encode += start.elapsed();
        ensure!(packed.len() <= raw.len(), "{} inflated the snapshot", self.name);
        let start = Instant::now();
        let mut batch = None;
        for message in messages::split(index, packed) {
            self.messages += 1;
            self.bytes += message.len() as u64;
            batch = self.decoder.decode(&message)?;
        }
        let batch = batch.context("Missing decoded benchmark picture")?;
        self.decode += start.elapsed();
        ensure!(
            self.decoder.last_snapshot() == Some(snapshot.as_slice()),
            "{} failed lossless round trip",
            self.name
        );
        let count = batch.polygon_count();
        self.polygons += count as u64;
        self.peak_polygons = self.peak_polygons.max(count);
        Ok(batch)
    }
}

/// Benchmarks a file or FFmpeg source at its selected resolution, geometry budget and cadence.
pub(crate) async fn run(input: &FileInput) -> Result<()> {
    let fps = input.fps;
    let budget = input.rate.budget(fps)?;
    let canvas = budget.canvas;
    let mut encoder = input.rate.encoder(crate::layers::InferenceMode::Offline)?;
    let mut source = Source::file(&input.source, input.input_format.as_deref(), fps, canvas)?;
    let trial = |name, mode| Trial::new(name, input.rate.packet_encoder(mode));
    let mut trials = vec![
        trial("off", Compression::Off),
        trial("spatial", Compression::Spatial),
        trial("predictive", Compression::Predictive),
        trial("compact", Compression::Compact),
    ];
    if budget.format() == WireFormat::LayeredPolygon {
        trials.push(trial("compact-motion", Compression::CompactMotion));
    } else {
        trials.push(trial("mesh-patch", Compression::MeshPatch));
    }
    let mut count = 0;
    let mut changed = 0;
    let mut last_body = Vec::new();
    let mut visual_encode = Duration::ZERO;
    let mut render = Duration::ZERO;
    let mut strokes = 0;
    let result = async {
        while let Some(frame) = source.read_frame(budget.format()).await? {
            let start = Instant::now();
            let raw = encoder.encode(count as u32, fps, &budget, &frame);
            visual_encode += start.elapsed();
            let body = &raw[crate::codec::header_len(raw[2])..];
            changed += usize::from(body != last_body);
            last_body.clear();
            last_body.extend_from_slice(body);
            let batch = trials[0].measure(count as u32, &raw)?;
            strokes += batch.stroke_count();
            let start = Instant::now();
            std::hint::black_box(batch.render());
            render += start.elapsed();
            for trial in &mut trials[1..] {
                trial.measure(count as u32, &raw)?;
            }
            count += 1;
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let stopped = source.stop().await;
    result.and(stopped)?;
    ensure!(count > 0, "Input contained no video frames");
    let seconds = fps.duration(count).as_secs_f64();
    let baseline = trials[0].bytes;
    println!("{count} frames, {count} snapshots, {}x{}, {fps} fps", canvas.width, canvas.height);
    println!(
        "{changed} mesh or layer snapshots with changed body bytes, including the first frame"
    );
    println!(
        "Visual encode {:.2} ms/frame, render {:.2} ms/frame",
        visual_encode.as_secs_f64() * 1000.0 / count as f64,
        render.as_secs_f64() * 1000.0 / count as f64
    );
    if trials[0].peak_polygons > 0 {
        println!(
            "{}/frame: {:.1} average, {} peak",
            match budget.format() {
                WireFormat::LayeredPolygon => "Painted layers",
                WireFormat::ColoredPolygon => "Visible triangles",
            },
            trials[0].polygons as f64 / count as f64,
            trials[0].peak_polygons
        );
    }
    if strokes != 0 {
        println!("Line strokes/frame: {:.1} average", strokes as f64 / count as f64);
    }
    println!(
        "compression    payload_kbps  modeled_kbps  saved_pct  pack_ms/snapshot  unpack_ms/snapshot  payload_bytes"
    );
    for t in trials {
        let kbps = |bytes: u64| bytes as f64 * 0.008 / seconds;
        let ms = |elapsed: Duration| elapsed.as_secs_f64() * 1000.0 / count as f64;
        println!(
            "{:<14} {:>12.2} {:>13.2} {:>10.1} {:>12.3} {:>14.3} {:>14}",
            t.name,
            kbps(t.bytes),
            kbps(t.bytes + t.messages * budget.overhead as u64),
            100.0 * (1.0 - t.bytes as f64 / baseline as f64),
            ms(t.encode),
            ms(t.decode),
            t.bytes
        );
    }
    Ok(())
}
