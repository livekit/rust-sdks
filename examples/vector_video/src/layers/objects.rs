//! Salient-object masks supply foreground geometry before color fitting.
#[cfg(all(target_os = "macos", feature = "apple-vision"))]
mod inset;
mod motion;
#[cfg(all(target_os = "macos", feature = "apple-vision"))]
mod vision;

use std::{path::Path, time::Instant};

use anyhow::{ensure, Context, Result};
use ort::{session::Session, tensor::TensorElementType, value::TensorRef};

use crate::video::Canvas;

use super::{
    inference::{axis, lerp, open_model, Worker},
    regions::{self, Silhouette},
    Edges, InferenceMode, ObjectProvider, Outline, H, PLANE, W,
};

const REFRESH: u8 = 3;
const MAX_RESULT_AGE: u8 = 6;
const MOTION_RADIUS: i32 = 5;
/// The motion reference and optional higher-resolution input owned by the worker.
#[derive(Default)]
struct Samples {
    coarse: Vec<[u8; 3]>,
    detail: Vec<[u8; 3]>,
}

/// A completed result and its input for motion alignment.
type Observation = (Samples, Result<Vec<Silhouette>>);

/// Supplies a binary foreground mask through a portable or platform model.
enum Foreground {
    Onnx(OnnxSegmenter),
    #[cfg(all(target_os = "macos", feature = "apple-vision"))]
    Vision(vision::Vision),
}

/// Fits the same polygon contours to either provider's foreground mask.
struct Segmenter {
    model: Foreground,
    refine: bool,
}

impl Segmenter {
    /// Opens and warms the chosen foreground provider.
    fn new(provider: ObjectProvider, path: Option<&Path>, refine: bool) -> Result<Self> {
        let model = match provider {
            ObjectProvider::Onnx => Foreground::Onnx(OnnxSegmenter::new(path)?),
            ObjectProvider::AppleVision => {
                #[cfg(all(target_os = "macos", feature = "apple-vision"))]
                {
                    Foreground::Vision(vision::Vision::new()?)
                }
                #[cfg(not(all(target_os = "macos", feature = "apple-vision")))]
                anyhow::bail!(
                    "--object-provider apple-vision requires macOS and --features apple-vision"
                );
            }
        };
        Ok(Self { model, refine })
    }

    /// Detects foreground and extracts quantized contours from the common fitting grid.
    fn detect(&mut self, rgb: &[[u8; 3]], _detail: &[[u8; 3]]) -> Result<Vec<Silhouette>> {
        let started = Instant::now();
        let outlines = match &mut self.model {
            Foreground::Onnx(model) => regions::silhouettes(&model.detect(rgb)?, self.refine),
            #[cfg(all(target_os = "macos", feature = "apple-vision"))]
            Foreground::Vision(model) => {
                let mask = model.detect(rgb, _detail)?;
                let mut outlines = regions::silhouettes(&mask, self.refine);
                if self.refine {
                    outlines.extend(inset::silhouettes(model, &mask, _detail));
                }
                outlines
            }
        };
        log::debug!(
            "Object segmentation took {:.2} ms and returned {} outlines",
            started.elapsed().as_secs_f64() * 1000.,
            outlines.len()
        );
        Ok(outlines)
    }
}

/// Runs a batch-one ONNX foreground segmenter on the model's fixed input grid.
struct OnnxSegmenter {
    session: Session,
    size: Canvas,
    xs: Vec<(usize, usize, f32)>,
    input: Vec<f32>,
}

impl OnnxSegmenter {
    /// Loads the bundled model or a user-supplied foreground graph for CPU inference.
    fn new(path: Option<&Path>) -> Result<Self> {
        const MODEL: &[u8] = include_bytes!("../../models/u2netp.onnx");
        let session = match path {
            Some(path) => open_model("Object segmentation", false, |builder| {
                Ok(builder.commit_from_file(path)?)
            })
            .with_context(|| format!("Could not load object model {}", path.display()))?,
            None => open_model("Object segmentation", true, |builder| {
                Ok(builder.commit_from_memory(MODEL)?)
            })?,
        };
        let size = Self::model_canvas(&session)?;
        let xs = (0..size.width).map(|x| axis(x, W, size.width)).collect();
        Ok(Self { session, size, xs, input: vec![0.; 3 * size.width * size.height] })
    }

    /// Validates the normalized RGB input and matching one-channel mask output.
    fn model_canvas(session: &Session) -> Result<Canvas> {
        let ([input], [output]) = (session.inputs(), session.outputs()) else {
            anyhow::bail!("Object model must have one input and one output");
        };
        let Some([1, 3, h, w]) = input.dtype().tensor_shape().map(|shape| &shape[..]) else {
            anyhow::bail!("Object model must accept a fixed [1, 3, H, W] RGB tensor");
        };
        ensure!(
            input.dtype().tensor_type() == Some(TensorElementType::Float32)
                && output.dtype().tensor_type() == Some(TensorElementType::Float32)
                && output.dtype().tensor_shape().map(|shape| &shape[..]) == Some(&[1, 1, *h, *w]),
            "Object model must accept float32 RGB and return a matching float32 [1, 1, H, W] mask"
        );
        ensure!(*h > 0 && *w > 0, "Object model must declare fixed, positive H and W");
        let (width, height) = (usize::try_from(*w)?, usize::try_from(*h)?);
        ensure!(
            width.checked_mul(height).is_some_and(|area| area <= Canvas::MAX_PIXELS),
            "Object model input grid must contain at most {} pixels",
            Canvas::MAX_PIXELS
        );
        Ok(Canvas { width, height })
    }

    /// Resamples the analysis plane and thresholds its foreground probabilities.
    fn detect(&mut self, rgb: &[[u8; 3]]) -> Result<Vec<bool>> {
        let Canvas { width, height } = self.size;
        for y in 0..height {
            let (t, b, fy) = axis(y, H, height);
            for (x, &(l, r, fx)) in self.xs.iter().enumerate() {
                let top = [rgb[t * W + l], rgb[t * W + r]];
                let bottom = [rgb[b * W + l], rgb[b * W + r]];
                for c in 0..3 {
                    let k =
                        |row: [[u8; 3]; 2]| lerp(f32::from(row[0][c]), f32::from(row[1][c]), fx);
                    let value = lerp(k(top), k(bottom), fy) / 255.;
                    self.input[(c * height + y) * width + x] =
                        (value - [0.485, 0.456, 0.406][c]) / [0.229, 0.224, 0.225][c];
                }
            }
        }
        let input = TensorRef::from_array_view(([1, 3, height, width], self.input.as_slice()))?;
        let output = self.session.run(ort::inputs![input])?;
        let (shape, values) = output[0].try_extract_tensor::<f32>()?;
        ensure!(shape.as_ref() == [1, 1, height as i64, width as i64], "Invalid object mask shape");
        Ok(threshold(values, self.size))
    }
}

/// Projects relative foreground confidence back to the analysis resolution.
fn threshold(values: &[f32], size: Canvas) -> Vec<bool> {
    let (min, max) = values.iter().fold((1.0f32, 0.0f32), |(lo, hi), &p| (lo.min(p), hi.max(p)));
    if max - min < 0.04 {
        return Vec::new();
    }
    let cutoff = lerp(min, max, 0.5);
    let xs = std::array::from_fn::<_, W, _>(|x| axis(x, size.width, W));
    let mut mask = Vec::with_capacity(W * H);
    for y in 0..H {
        let (t, b, fy) = axis(y, size.height, H);
        for &(l, r, fx) in &xs {
            let a = lerp(values[t * size.width + l], values[t * size.width + r], fx);
            let b = lerp(values[b * size.width + l], values[b * size.width + r], fx);
            mask.push(lerp(a, b, fy) >= cutoff);
        }
    }
    mask
}

/// Estimates motion from pixels inside a contour, using a sparse grid to bound the search cost.
fn follow(silhouette: &mut Silhouette, old: &[[u8; 3]], new: &[[u8; 3]], refine: bool) {
    let edges = Edges::new(&silhouette.outline, PLANE);
    let margin = MOTION_RADIUS as usize;
    let mut points = Vec::new();
    for y in (edges.top.max(margin)..edges.bottom.min(H - margin)).step_by(6) {
        edges.spans(y, |l, r| {
            let (l, r) = (l.max(margin), r.min(W - margin));
            points.extend((l..r).step_by(6).map(|x| (x as i32, y as i32)));
        });
    }
    if points.is_empty() {
        return;
    }
    let mut best = (u32::MAX, 0, 0);
    for dy in -MOTION_RADIUS..=MOTION_RADIUS {
        for dx in -MOTION_RADIUS..=MOTION_RADIUS {
            let mut cost = (dx.unsigned_abs() + dy.unsigned_abs()) * points.len() as u32 / 2;
            for &(x, y) in &points {
                let a = old[y as usize * W + x as usize];
                let b = new[(y + dy) as usize * W + (x + dx) as usize];
                cost += (0..3).map(|c| u32::from(a[c].abs_diff(b[c]))).sum::<u32>();
            }
            if cost < best.0 {
                best = (cost, dx, dy);
            }
        }
    }
    let movement = (best.1, best.2);
    translate(&mut silhouette.outline, movement);
    let refine = refine && silhouette.parts.len() + silhouette.extensions.len() > 1;
    for part in silhouette.parts.iter_mut().chain(&mut silhouette.extensions) {
        let movement = if refine { refine_motion(part, old, new, movement) } else { movement };
        translate(part, movement);
    }
}

/// Translates a wire outline by a displacement measured on the coarse analysis grid.
fn translate(outline: &mut Outline, (dx, dy): (i32, i32)) {
    let dx = (dx as f32 * 255. / W as f32).round() as i32;
    let dy = (dy as f32 * 255. / H as f32).round() as i32;
    for [x, y] in outline {
        *x = (i32::from(*x) + dx).clamp(0, 255) as u8;
        *y = (i32::from(*y) + dy).clamp(0, 255) as u8;
    }
}

/// Refines the common shift when a contour has enough pixels that match better nearby.
fn refine_motion(
    outline: &Outline,
    old: &[[u8; 3]],
    new: &[[u8; 3]],
    (dx, dy): (i32, i32),
) -> (i32, i32) {
    const RADIUS: i32 = 2;
    const MAX_SAMPLES: usize = 56;
    let margin = (MOTION_RADIUS + RADIUS) as usize;
    let edges = Edges::new(outline, PLANE);
    let spacing = if edges.bottom.saturating_sub(edges.top) < 18 { 2 } else { 4 };
    let mut points = Vec::new();
    for y in (edges.top.max(margin)..edges.bottom.min(H - margin)).step_by(spacing) {
        edges.spans(y, |l, r| {
            points.extend(
                (l.max(margin)..r.min(W - margin)).step_by(spacing).map(|x| (x, y, old[y * W + x])),
            );
        });
    }
    if points.len() < 6 {
        return (dx, dy);
    }
    let step = points.len().div_ceil(MAX_SAMPLES);
    let count = points.len().div_ceil(step) as u32;
    let cost = |ox: i32, oy: i32| {
        let offset = 4 * count * (ox.unsigned_abs() + oy.unsigned_abs());
        points.iter().step_by(step).fold(offset, |cost, &(x, y, before)| {
            let after = new[(y as i32 + dy + oy) as usize * W + (x as i32 + dx + ox) as usize];
            cost + before
                .into_iter()
                .zip(after)
                .map(|(a, b)| u32::from(a.abs_diff(b).min(48)))
                .sum::<u32>()
        })
    };
    let common = cost(0, 0);
    let minimum = common / 5 + 3 * count;
    let mut best = (common, dx, dy);
    for ox in -RADIUS..=RADIUS {
        for oy in -RADIUS..=RADIUS {
            if ox == 0 && oy == 0 {
                continue;
            }
            let local = cost(ox, oy);
            if local < best.0 && common - local >= minimum {
                best = (local, dx + ox, dy + oy);
            }
        }
    }
    (best.1, best.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Moves a textured part independently and keeps the common shift on a flat patch.
    #[test]
    fn follows_part_only_with_local_evidence() {
        let outline = [
            [64, 64],
            [112, 64],
            [112, 104],
            [64, 104],
            [64, 104],
            [64, 104],
            [64, 104],
            [64, 104],
        ];
        let flat = vec![[90; 3]; W * H];
        assert_eq!(refine_motion(&outline, &flat, &flat, (0, 0)), (0, 0));

        let previous: Vec<_> = (0..W * H)
            .map(|i| {
                let value = (((i % W) * 57) ^ ((i / W) * 91)) as u8;
                [value, value.rotate_left(3), value.rotate_left(5)]
            })
            .collect();
        let mut current = flat;
        for (row, before) in current.chunks_exact_mut(W).zip(previous.chunks_exact(W)) {
            row[2..].copy_from_slice(&before[..W - 2]);
        }
        assert_eq!(refine_motion(&outline, &previous, &current, (0, 0)), (2, 0));
    }
}

/// Keeps the input allocation and tracks the age of the one live model request.
struct Live {
    worker: Worker<Samples, Observation>,
    buffer: Samples,
    use_detail: bool,
    age: u8,
}

/// Chooses between inline preview inference and an asynchronous live detector.
enum Inference {
    Inline(Box<Segmenter>),
    Live(Live),
}

/// Retains the subject outlines between neural observations.
#[derive(Default)]
pub(super) struct Objects {
    silhouettes: Vec<Silhouette>,
    inference: Option<Inference>,
    refine: bool,
    until_detect: u8,
    expired: u8,
    failed: bool,
}

impl Objects {
    /// Supplies a fixed segmentation mask to tests without loading an inference runtime.
    #[cfg(test)]
    pub(super) fn with_silhouettes(mask: &[bool]) -> Self {
        Self { silhouettes: regions::silhouettes(mask, true), ..Self::default() }
    }

    /// Sets up deterministic offline inference or a bounded worker for live publication.
    pub(super) fn new(
        mode: InferenceMode,
        provider: ObjectProvider,
        path: Option<&Path>,
        refine: bool,
    ) -> Result<Self> {
        let inference = match mode {
            InferenceMode::Disabled => return Ok(Self::default()),
            InferenceMode::Offline => {
                Inference::Inline(Box::new(Segmenter::new(provider, path, refine)?))
            }
            InferenceMode::Live => {
                let mut model = Segmenter::new(provider, path, refine)?;
                let worker = Worker::new("vector-object-segmentation", move |input: Samples| {
                    let outlines = model.detect(&input.coarse, &input.detail);
                    (input, outlines)
                })?;
                Inference::Live(Live {
                    worker,
                    buffer: Samples::default(),
                    use_detail: provider == ObjectProvider::AppleVision,
                    age: 0,
                })
            }
        };
        Ok(Self { inference: Some(inference), refine, ..Default::default() })
    }

    /// Allocates at most one quarter of the paint slots to object contours.
    pub(super) fn outlines(&self, layer_limit: usize, refine: bool) -> Vec<Outline> {
        let cap = (layer_limit / 4).clamp(1, if refine { 6 } else { 4 }).min(layer_limit);
        let subjects = self.silhouettes.len().min(cap);
        let mut outlines = Vec::new();
        let mut extensions = Vec::new();
        for (i, subject) in self.silhouettes[..subjects].iter().enumerate() {
            let available = cap - outlines.len() - (subjects - i - 1);
            if subject.parts.len() > available {
                outlines.push(subject.outline);
            } else {
                if subject.parts.is_empty() {
                    outlines.push(subject.outline);
                } else {
                    outlines.extend_from_slice(&subject.parts);
                }
                extensions.extend_from_slice(&subject.extensions);
            }
        }
        outlines.extend(extensions.into_iter().take(cap - outlines.len()));
        outlines
    }

    /// Refreshes outlines, translates them between model frames, and discards results across cuts.
    pub(super) fn update(
        &mut self,
        old: &[[u8; 3]],
        rgb: &[[u8; 3]],
        detail: &[[u8; 3]],
        cut: bool,
        changed: bool,
    ) -> bool {
        let Some(engine) = &mut self.inference else { return false };
        let refine = self.refine;
        // Large pans can reset the paint fit while the moving subjects are still trackable.
        let pan = cut && !old.is_empty() && motion::coherent(old, rgb);
        let cut = cut && !pan;
        if cut {
            self.silhouettes.clear();
            self.until_detect = 0;
        }
        let observation = match engine {
            Inference::Inline(model) if self.until_detect == 0 && changed => {
                self.until_detect = REFRESH;
                model.detect(rgb, detail).map(Some)
            }
            Inference::Inline(_) => Ok(None),
            Inference::Live(live) => {
                // A cut invalidates outstanding input even if the old frame was static.
                live.age = if cut { u8::MAX } else { live.age.saturating_add(u8::from(changed)) };
                let refresh_one_frame_early = pan && live.use_detail;
                live.worker.poll().and_then(|ready| {
                    let mut observation = None;
                    if let Some((input, result)) = ready {
                        if live.age <= MAX_RESULT_AGE {
                            let mut outlines = result?;
                            if input.coarse != rgb {
                                for outline in &mut outlines {
                                    follow(outline, &input.coarse, rgb, refine);
                                }
                            }
                            observation = Some(outlines);
                        }
                        live.buffer = input;
                    }
                    if self.until_detect <= u8::from(refresh_one_frame_early)
                        && !live.worker.busy()
                        && (changed || cut || live.age > MAX_RESULT_AGE)
                    {
                        live.buffer.coarse.clear();
                        live.buffer.coarse.extend_from_slice(rgb);
                        if live.use_detail {
                            live.buffer.detail.clear();
                            live.buffer.detail.extend_from_slice(detail);
                        }
                        live.worker.submit(std::mem::take(&mut live.buffer))?;
                        live.age = 0;
                        self.until_detect = REFRESH;
                    }
                    Ok(observation)
                })
            }
        };
        if changed {
            self.until_detect = self.until_detect.saturating_sub(1);
            self.expired = self.expired.saturating_add(1);
        }
        match observation {
            Ok(Some(outlines)) => {
                self.silhouettes = outlines;
                self.expired = 0;
                self.failed = false;
                true
            }
            Ok(None) if self.expired > MAX_RESULT_AGE => {
                let had_outlines = !self.silhouettes.is_empty();
                self.silhouettes.clear();
                had_outlines
            }
            Ok(None) => {
                if changed && !cut {
                    for outline in &mut self.silhouettes {
                        follow(outline, old, rgb, refine);
                    }
                }
                false
            }
            Err(error) => {
                if !self.failed {
                    eprintln!("Object segmentation failed: {error:#}. Using image-derived layers.");
                }
                self.failed = true;
                self.silhouettes.clear();
                self.until_detect = 30;
                true
            }
        }
    }
}
