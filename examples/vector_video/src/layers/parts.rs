//! Compares the source and the draft paint to find missing structure at any object class.
use std::{path::Path, time::Instant};

use anyhow::{ensure, Context, Result};
use ort::{session::Session, value::TensorRef};

use super::{
    fit::luma,
    inference::{axis, lerp, open_model, Worker},
    unpack, InferenceMode, Scene, DETAIL_PLANE, H, W,
};

const WIDTH: usize = 224;
const HEIGHT: usize = 126;
const GRID: [usize; 2] = [9, 16];
const CELLS: usize = GRID[0] * GRID[1];
const INPUT_LEN: usize = 2 * 3 * WIDTH * HEIGHT;
const REFRESH: u64 = 6;
const MAX_AGE: u64 = 2 * REFRESH;
pub(super) const MAX_MARKS: usize = 20;
type PatchGrid = [f32; CELLS];

/// Patch-level guidance for missing detail and optional foreground importance.
struct Prediction {
    loss: PatchGrid,
    foreground: Option<PatchGrid>,
}

/// A job owns its input until the worker returns the same allocation.
struct Job {
    frame: u64,
    input: Vec<f32>,
}

/// A model observation and the input allocation it no longer needs.
struct Observation {
    job: Job,
    prediction: Result<Prediction>,
}

/// Runs in the chosen observation mode while owning the inference session.
enum Engine {
    Inline(Session),
    Live(Worker<Job, Observation>),
}

/// Maintains a bounded stream of model observations and their current detail regions.
pub(super) struct Parts {
    engine: Engine,
    frame: u64,
    cut: u64,
    next: u64,
    observed: Option<u64>,
    input: Vec<f32>,
    ready: Option<PatchGrid>,
    labels: Vec<u8>,
    focus: Vec<bool>,
    focus_changed: bool,
    failed: bool,
}

impl Parts {
    /// Loads and verifies the model, then prepares synchronous or bounded live inference.
    pub(super) fn new(path: &Path, mode: InferenceMode) -> Result<Self> {
        let mut session =
            open_model("Part guidance", true, |builder| Ok(builder.commit_from_file(path)?))
                .with_context(|| format!("Could not load part model {}", path.display()))?;
        let input = vec![0.; INPUT_LEN];
        predict(&mut session, &input).context(
            "Part model must accept [2, 3, 126, 224] and return a 9×16 loss grid with optional foreground guidance",
        )?;
        let engine = match mode {
            InferenceMode::Offline => Engine::Inline(session),
            InferenceMode::Live => {
                Engine::Live(Worker::new("vector-part-guidance", move |job: Job| {
                    let prediction = predict(&mut session, &job.input);
                    Observation { job, prediction }
                })?)
            }
            InferenceMode::Disabled => anyhow::bail!("Part guidance requires an inference mode"),
        };
        Ok(Self {
            engine,
            frame: 0,
            cut: 0,
            next: 0,
            observed: None,
            input,
            ready: None,
            labels: Vec::new(),
            focus: Vec::new(),
            focus_changed: false,
            failed: false,
        })
    }

    /// Discards stale results and reports changes even when the source stays still.
    pub(super) fn advance(&mut self, cut: bool, changed: bool) -> bool {
        self.frame += u64::from(changed);
        let pending = std::mem::take(&mut self.focus_changed);
        if cut {
            self.cut = self.frame;
            self.next = self.frame;
            self.observed = None;
            self.ready = None;
            self.labels.clear();
            self.focus.clear();
        }
        let expired = self.observed.is_some_and(|frame| self.frame - frame > MAX_AGE);
        if expired {
            self.observed = None;
            self.ready = None;
            self.labels.clear();
            self.focus.clear();
        }
        let result = match &mut self.engine {
            Engine::Inline(_) => Ok(None),
            Engine::Live(worker) => worker.poll(),
        };
        match result {
            Ok(Some(observation)) => self.accept(observation) || expired || pending,
            Ok(None) => expired || pending,
            Err(error) => {
                self.report_error(&error);
                expired || pending
            }
        }
    }

    /// Returns the pixels whose shapes should receive extra weight in the broad fit.
    pub(super) fn focus(&self) -> &[bool] {
        &self.focus
    }

    /// Starts the next pair comparison when due and projects new guidance onto the detail grid.
    pub(super) fn hints(
        &mut self,
        rgb: &[[u8; 3]],
        owners: &[u16],
        scene: &Scene,
        subject: &[bool],
        signal: &[i16],
    ) -> &[u8] {
        let available = match &self.engine {
            Engine::Inline(_) => true,
            Engine::Live(worker) => !worker.busy(),
        };
        if available && self.frame >= self.next {
            self.next = self.frame + REFRESH;
            let colors: Vec<_> = std::iter::once(scene.background)
                .chain(scene.patches.iter().map(|patch| patch.color))
                .map(unpack)
                .collect();
            sample_pair(&mut self.input, rgb, owners, &colors);
            let job = Job { frame: self.frame, input: std::mem::take(&mut self.input) };
            match &mut self.engine {
                Engine::Inline(session) => {
                    let prediction = predict(session, &job.input);
                    let has_focus =
                        prediction.as_ref().is_ok_and(|result| result.foreground.is_some());
                    self.focus_changed = self.accept(Observation { job, prediction }) && has_focus;
                }
                Engine::Live(worker) => {
                    if let Err(error) = worker.submit(job) {
                        self.report_error(&error);
                    }
                }
            }
        }
        if let Some(loss) = self.ready.take() {
            // Coarse model scores protect small subjects whose details have weak local contrast.
            if self.focus.iter().filter(|&&covered| covered).count() >= W * H / 8 {
                let lumas: Vec<_> = std::iter::once(scene.background)
                    .chain(scene.patches.iter().map(|patch| patch.color))
                    .map(|color| luma(unpack(color)) as i16)
                    .collect();
                locate(
                    &loss,
                    subject,
                    |i| {
                        let ridge = signal[i];
                        let missing = luma(rgb[i]) as i16 - lumas[usize::from(owners[i])];
                        let strength = if ridge.signum() == missing.signum() {
                            ridge.abs().min(30) * 2 + missing.abs().min(24)
                        } else {
                            0
                        };
                        0.65 + 0.35 * (f32::from(strength) / 84.)
                    },
                    &mut self.labels,
                );
            } else {
                locate(&loss, subject, |_| 1., &mut self.labels);
            }
        }
        &self.labels
    }

    /// Recycles the input buffer and accepts only observations from this scene.
    fn accept(&mut self, observation: Observation) -> bool {
        let Observation { job, prediction } = observation;
        self.input = job.input;
        if job.frame < self.cut || self.frame - job.frame > MAX_AGE {
            return false;
        }
        match prediction {
            Ok(result) => {
                self.observed = Some(job.frame);
                self.ready = Some(result.loss);
                self.focus.clear();
                if let Some(probability) = result.foreground {
                    project_focus(&probability, &mut self.focus);
                }
                self.failed = false;
                true
            }
            Err(error) => {
                self.report_error(&error);
                false
            }
        }
    }

    /// Reports a failure once while the image-derived fitter continues.
    fn report_error(&mut self, error: &anyhow::Error) {
        if !self.failed {
            log::warn!("Part guidance failed: {error:#}");
            self.failed = true;
        }
    }
}

/// Reads detail loss and optional foreground probabilities from the same model pass.
fn predict(session: &mut Session, input: &[f32]) -> Result<Prediction> {
    let start = Instant::now();
    let tensor = TensorRef::from_array_view(([2, 3, HEIGHT, WIDTH], input))?;
    let outputs = session.run(ort::inputs![tensor])?;
    ensure!((1..=2).contains(&outputs.len()), "Invalid part model output count");
    let (shape, values) = outputs[0].try_extract_tensor::<f32>()?;
    ensure!(
        shape.as_ref() == GRID.map(|n| n as i64) && values.iter().all(|v| v.is_finite()),
        "Invalid part model loss grid"
    );
    let loss = values.try_into()?;
    let foreground = if outputs.len() == 2 {
        let (shape, values) = outputs[1].try_extract_tensor::<f32>()?;
        ensure!(
            shape.as_ref() == [1, 1, GRID[0] as i64, GRID[1] as i64]
                && values.iter().all(|v| (0.0..=1.0).contains(v)),
            "Invalid part model foreground probabilities"
        );
        Some(values.try_into()?)
    } else {
        None
    };
    log::debug!("Part guidance inference took {:.2} ms", start.elapsed().as_secs_f64() * 1000.);
    Ok(Prediction { loss, foreground })
}

/// Interpolates the model patch grid at the fitter's sample positions.
fn project_focus(probability: &PatchGrid, focus: &mut Vec<bool>) {
    const MIN_PROBABILITY: f32 = 96.0 / 255.0;
    let xs = std::array::from_fn::<_, W, _>(|x| axis(2 * x + 1, GRID[1], DETAIL_PLANE.width));
    for y in 0..H {
        let (t, b, fy) = axis(2 * y + 1, GRID[0], DETAIL_PLANE.height);
        focus.extend(xs.iter().map(|&(l, r, fx)| {
            let a = lerp(probability[t * GRID[1] + l], probability[t * GRID[1] + r], fx);
            let b = lerp(probability[b * GRID[1] + l], probability[b * GRID[1] + r], fx);
            lerp(a, b, fy) >= MIN_PROBABILITY
        }));
    }
}

/// Samples both model inputs directly from the fine plane and its resolved draft fills.
fn sample_pair(input: &mut Vec<f32>, rgb: &[[u8; 3]], owners: &[u16], colors: &[[u8; 3]]) {
    let plane = DETAIL_PLANE;
    let plane_len = WIDTH * HEIGHT;
    input.resize(INPUT_LEN, 0.);
    let xs = std::array::from_fn::<_, WIDTH, _>(|x| axis(x, plane.width, WIDTH));
    for y in 0..HEIGHT {
        let (t, b, fy) = axis(y, plane.height, HEIGHT);
        for (x, &(l, r, fx)) in xs.iter().enumerate() {
            let indices = [
                t * plane.width + l,
                t * plane.width + r,
                b * plane.width + l,
                b * plane.width + r,
            ];
            let pictures =
                [indices.map(|i| rgb[i]), indices.map(|i| colors[usize::from(owners[i])])];
            for (picture, patch) in pictures.into_iter().enumerate() {
                for (channel, (mean, dev)) in
                    [(0.485, 0.229), (0.456, 0.224), (0.406, 0.225)].into_iter().enumerate()
                {
                    let [a, b, c, d] = patch.map(|v| f32::from(v[channel]));
                    let v = lerp(lerp(a, b, fx), lerp(c, d, fx), fy).round();
                    input[(3 * picture + channel) * plane_len + y * WIDTH + x] =
                        (v / 255. - mean) / dev;
                }
            }
        }
    }
}

/// Returns the leftmost positive maximum in a row, or zero when no mark remains.
fn row_peak(scores: &[f32]) -> (usize, f32) {
    let mut peak = (0, 0.);
    for (x, &score) in scores.iter().enumerate() {
        if score > peak.1 {
            peak = (x, score);
        }
    }
    peak
}

/// Selects separated high-loss neighborhoods, with preference for an available subject outline.
fn locate(
    loss: &PatchGrid,
    subject: &[bool],
    contrast_weight: impl Fn(usize) -> f32,
    labels: &mut Vec<u8>,
) {
    const RADIUS: usize = 8;
    let plane = DETAIL_PLANE;
    labels.clear();
    let (lo, hi) =
        loss.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    if hi - lo <= 1e-5 {
        return;
    }
    let mut near = Vec::new();
    if !subject.is_empty() {
        near.resize(W * H, false);
        for (i, _) in subject.iter().enumerate().filter(|(_, &covered)| covered) {
            let (x, y) = (i % W, i / W);
            for row in y.saturating_sub(2)..=(y + 2).min(H - 1) {
                near[row * W + x.saturating_sub(2)..=row * W + (x + 2).min(W - 1)].fill(true);
            }
        }
    }
    let xs = std::array::from_fn::<_, { DETAIL_PLANE.width }, _>(|x| axis(x, GRID[1], plane.width));
    let mut scores = vec![0.; plane.width * plane.height];
    for y in RADIUS..plane.height - RADIUS {
        let (t, b, fy) = axis(y, GRID[0], plane.height);
        for (x, &(l, r, fx)) in xs.iter().enumerate().take(plane.width - RADIUS).skip(RADIUS) {
            let a = lerp(loss[t * GRID[1] + l], loss[t * GRID[1] + r], fx);
            let b = lerp(loss[b * GRID[1] + l], loss[b * GRID[1] + r], fx);
            let weight = if near.is_empty() || near[y / 2 * W + x / 2] { 1. } else { 0.18 };
            let i = y * plane.width + x;
            scores[i] = lerp(a, b, fy) * weight * contrast_weight(i);
        }
    }
    labels.resize(scores.len(), 0);
    // A suppressed neighborhood changes a row's maximum only if it contains that maximum.
    let mut peaks: Vec<_> = scores.chunks_exact(plane.width).map(row_peak).collect();
    for label in 1..=MAX_MARKS as u8 {
        let mut best = 0.;
        let (mut x, mut y) = (0, 0);
        for (row, &(column, score)) in peaks.iter().enumerate() {
            if score > best {
                best = score;
                (x, y) = (column, row);
            }
        }
        if best == 0. {
            break;
        }
        let columns = x - RADIUS..=x + RADIUS;
        for (row, peak) in peaks.iter_mut().enumerate().take(y + RADIUS + 1).skip(y - RADIUS) {
            let start = row * plane.width;
            let row_scores = &mut scores[start..start + plane.width];
            row_scores[columns.clone()].fill(0.);
            if columns.contains(&peak.0) {
                *peak = row_peak(row_scores);
            }
            let row_labels = &mut labels[start..start + plane.width];
            for (dx, value) in row_labels[columns.clone()].iter_mut().enumerate() {
                if *value == 0
                    && dx.abs_diff(RADIUS).pow(2) + row.abs_diff(y).pow(2) <= RADIUS.pow(2)
                {
                    *value = label;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };

    use super::*;

    /// A confident patch weights the nearby fitting pixels and leaves distant pixels alone.
    #[test]
    fn projects_foreground_guidance_to_the_fitting_plane() {
        let mut probability = [0.0; CELLS];
        probability[4 * GRID[1] + 7] = 1.0;
        let mut focus = Vec::new();
        project_focus(&probability, &mut focus);
        assert_eq!(focus.len(), W * H);
        assert!(focus[53 * W + 89]);
        assert!(!focus[53 * W + 120]);
        assert!(!focus[0]);
    }

    /// Confirms that the strongest useful region follows the foreground and flat loss adds none.
    #[test]
    fn ranks_separated_foreground_loss() {
        let mut labels = vec![1];
        locate(&[0.5; CELLS], &[], |_| 1., &mut labels);
        assert!(labels.is_empty());

        let mut loss = [0.; CELLS];
        loss[3 * GRID[1] + 6] = 1.;
        loss[4 * GRID[1] + 12] = 3.;
        let mut subject = vec![false; W * H];
        for y in 32..52 {
            subject[y * W + 65..y * W + 90].fill(true);
        }
        locate(&loss, &subject, |_| 1., &mut labels);
        assert_eq!(labels[84 * DETAIL_PLANE.width + 156], 1);
    }

    /// Clearing a winning neighborhood exposes the next peak without changing tie order.
    #[test]
    fn finds_remaining_row_peaks_after_suppression() {
        let width = DETAIL_PLANE.width;
        let points = [30 * width + 32, 30 * width + 37, 30 * width + 57, 55 * width + 22];
        let mut loss = [0.5; CELLS];
        loss[CELLS - 1] = 0.;
        let mut labels = Vec::new();
        locate(&loss, &[], |i| f32::from(u8::from(points.contains(&i))), &mut labels);
        assert_eq!(points.map(|i| labels[i]), [1, 1, 2, 3]);
        assert_eq!(labels.iter().copied().max(), Some(3));
    }

    /// Exercises submission, scene-cut invalidation and joining an outstanding worker.
    #[test]
    fn discards_cut_results_and_joins_in_flight_work() -> Result<()> {
        struct Lease(Arc<AtomicBool>);
        impl Drop for Lease {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }

        let size = DETAIL_PLANE.width * DETAIL_PLANE.height;
        let owners = vec![0; size];
        let rgb = vec![[180; 3]; size];
        for _ in 0..3 {
            let joined = Arc::new(AtomicBool::new(false));
            let completed = Arc::new(AtomicUsize::new(0));
            let lease = Lease(Arc::clone(&joined));
            let updates = Arc::clone(&completed);
            let worker = Worker::new("test-part-guidance", move |job| {
                let _owner = &lease;
                updates.fetch_add(1, Ordering::Relaxed);
                Observation {
                    job,
                    prediction: Ok(Prediction { loss: [0.5; CELLS], foreground: None }),
                }
            })?;
            let mut parts = Parts {
                engine: Engine::Live(worker),
                frame: 1,
                cut: 1,
                next: 1,
                observed: None,
                input: Vec::new(),
                ready: None,
                labels: Vec::new(),
                focus: Vec::new(),
                focus_changed: false,
                failed: false,
            };
            parts.hints(&rgb, &owners, &Scene::default(), &[], &[]);
            parts.advance(true, true);
            assert!(!parts.accept(Observation {
                job: Job { frame: 1, input: Vec::new() },
                prediction: Ok(Prediction { loss: [1.; CELLS], foreground: Some([1.; CELLS]) }),
            }));
            assert!(parts.labels.is_empty());
            assert!(parts.ready.is_none());
            assert!(parts.focus().is_empty());
            assert!(parts.accept(Observation {
                job: Job { frame: parts.frame, input: Vec::new() },
                prediction: Ok(Prediction { loss: [2.; CELLS], foreground: Some([0.5; CELLS]) }),
            }));
            assert!(parts.ready.is_some());
            parts.frame += MAX_AGE;
            assert!(parts.advance(false, true));
            assert!(parts.ready.is_none());
            drop(parts);
            assert_eq!(completed.load(Ordering::Relaxed), 1);
            assert!(joined.load(Ordering::Relaxed));
        }
        Ok(())
    }
}
