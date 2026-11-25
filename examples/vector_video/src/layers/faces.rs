//! Optional sender-only face guidance for layered shapes.
mod detector;
mod worker;

use super::{Canvas, Edges, InferenceMode, Outline, H, W};

/// Face silhouette, nose, mouth, eyelids, brows, and irises in normalized picture coordinates.
#[derive(Clone, Copy, Default)]
pub(super) struct Guide {
    pub(super) parts: [Outline; 9],
}

impl Guide {
    /// Returns the face bounds on the analysis raster for local weighting and optical tracking.
    pub(super) fn bounds(&self) -> [usize; 4] {
        let xy = self.parts[0];
        let xs = xy.map(|p| usize::from(p[0]) * W / 255);
        let ys = xy.map(|p| usize::from(p[1]) * H / 255);
        [
            *xs.iter().min().expect("eight vertices"),
            *ys.iter().min().expect("eight vertices"),
            *xs.iter().max().expect("eight vertices"),
            *ys.iter().max().expect("eight vertices"),
        ]
    }

    /// Moves all facial parts together using a translation estimated between detector frames.
    fn follow(&mut self, old: &[[u8; 3]], new: &[[u8; 3]]) {
        let [x0, y0, x1, y1] = self.bounds();
        let (x0, y0, x1, y1) = (x0.max(3), y0.max(3), x1.min(W - 3), y1.min(H - 3));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let mut best = (u32::MAX, 0, 0);
        for dy in -3i32..=3 {
            for dx in -3i32..=3 {
                let mut cost = (dx.unsigned_abs() + dy.unsigned_abs()) * 8;
                for y in (y0..y1).step_by(2) {
                    for x in (x0..x1).step_by(2) {
                        let a = old[y * W + x];
                        let b = new[(y as i32 + dy) as usize * W + (x as i32 + dx) as usize];
                        cost += (0..3).map(|c| u32::from(a[c].abs_diff(b[c]))).sum::<u32>();
                    }
                }
                if cost < best.0 {
                    best = (cost, dx, dy);
                }
            }
        }
        let delta = [
            (best.1 as f32 * 255. / W as f32).round() as i32,
            (best.2 as f32 * 255. / H as f32).round() as i32,
        ];
        for ring in &mut self.parts {
            if ring.iter().all(|p| *p == [0, 0]) {
                continue;
            }
            for p in ring {
                for i in 0..2 {
                    p[i] = (i32::from(p[i]) + delta[i]).clamp(0, 255) as u8;
                }
            }
        }
    }
}

/// Z positions reserved for silhouettes and their foreground details.
pub(super) struct Layout<'a> {
    faces: &'a [Guide],
    base_start: usize,
    pub(super) detail_start: usize,
    pub(super) features: Vec<(usize, Outline)>,
}
impl<'a> Layout<'a> {
    /// Reserves at most half the paint slots for landmarks, distributing each feature across faces.
    pub(super) fn new(guides: &'a [Guide], limit: usize) -> Self {
        // Leave room for the whole scene even when several faces are visible.
        let face_slots =
            if guides.is_empty() { 0 } else { (limit / 2).max(1).min(9 * guides.len()) };
        let guide_count = guides.len().min(face_slots);
        let general = limit - face_slots;
        let base_start = general / 2;
        let detail_start = general + guide_count;
        let mut features = Vec::new();
        for part in 1..9 {
            for guide in &guides[..guide_count] {
                if features.len() < face_slots - guide_count {
                    features.push((part, guide.parts[part]));
                }
            }
        }
        Self { faces: &guides[..guide_count], base_start, detail_start, features }
    }

    /// Gives the outline for a reserved slot, leaving the rest for image-derived regions.
    pub(super) fn outline(&self, i: usize) -> Option<Outline> {
        if i >= self.detail_start {
            return self.features.get(i - self.detail_start).map(|(_, p)| *p);
        }
        self.faces.get(i.checked_sub(self.base_start)?).map(|g| g.parts[0])
    }

    /// Maintains stable landmark positions when a flat scene needs fewer background polygons.
    pub(super) fn needs_padding(&self, i: usize) -> bool {
        i < self.detail_start && !self.faces.is_empty()
    }
}

/// Offline processing waits for each scheduled observation. Live capture owns one detector thread.
enum Inference {
    Inline(Box<detector::Detector>),
    Live(worker::Worker),
}

/// Cached observations used between detector frames.
#[derive(Default)]
pub(super) struct Faces {
    pub(super) guides: Vec<Guide>,
    inference: Option<Inference>,
    until_detect: u8,
    failed: bool,
}

impl Faces {
    /// Loads the CPU models only when the encoder requests face guidance.
    pub(super) fn new(mode: InferenceMode) -> anyhow::Result<Self> {
        let inference = match mode {
            InferenceMode::Disabled => return Ok(Self::default()),
            InferenceMode::Offline => Inference::Inline(Box::new(detector::Detector::new()?)),
            InferenceMode::Live => {
                let detector = detector::Detector::new()?;
                Inference::Live(worker::Worker::new(move |rgb, size| detector.detect(rgb, size))?)
            }
        };
        Ok(Self { inference: Some(inference), ..Default::default() })
    }

    /// Polls live observations even for still pictures, discarding outstanding work across cuts.
    pub(super) fn update(
        &mut self,
        input: &[u8],
        size: Canvas,
        old: &[[u8; 3]],
        new: &[[u8; 3]],
        cut: bool,
        changed: bool,
    ) -> bool {
        let Some(engine) = &mut self.inference else { return false };
        if cut {
            self.guides.clear();
            self.until_detect = 0;
            if let Inference::Live(worker) = engine {
                worker.invalidate();
            }
        }
        let due = self.until_detect == 0;
        let observation = match engine {
            Inference::Inline(detector) if due && changed => {
                self.until_detect = 3;
                detector.detect(input, size).map(Some)
            }
            Inference::Inline(_) => Ok(None),
            Inference::Live(worker) => worker.poll(new).and_then(|ready| {
                if ready.is_none() && due && !worker.busy() {
                    worker.submit(input, size, new)?;
                    self.until_detect = 3;
                }
                Ok(ready)
            }),
        };
        if changed {
            self.until_detect = self.until_detect.saturating_sub(1);
        }
        match observation {
            Ok(Some(guides)) => {
                self.guides = guides;
                self.failed = false;
                true
            }
            Ok(None) => {
                if changed && !cut {
                    for g in &mut self.guides {
                        g.follow(old, new);
                    }
                }
                false
            }
            Err(error) => {
                if !self.failed {
                    eprintln!(
                        "Face landmark inference failed: {error:#}. Using image-derived layers."
                    );
                }
                self.failed = true;
                self.guides.clear();
                self.until_detect = 29;
                true
            }
        }
    }
}

/// Fits small landmark colors at source resolution, favoring the dark core of brows and irises.
pub(super) fn feature_color(edges: &Edges, part: usize, input: &[u8], size: Canvas) -> Option<u16> {
    let mut bins = [[0u64; 4]; 16];
    for y in edges.top..edges.bottom {
        edges.spans(y, |l, r| {
            for x in l..r {
                let i = 3 * (y * size.width + x);
                let (r, g, b) =
                    (u64::from(input[i]), u64::from(input[i + 1]), u64::from(input[i + 2]));
                let bin = &mut bins[((r + 2 * g + b) / 64) as usize];
                for (s, value) in bin.iter_mut().zip([r, g, b, 1]) {
                    *s += value;
                }
            }
        });
    }
    let count: u64 = bins.iter().map(|b| b[3]).sum();
    if count == 0 {
        return None;
    }
    let n = if part >= 5 { count.div_ceil(2) } else { count };
    let mut left = n;
    let mut sum = [0u64; 3];
    for bin in bins.iter().filter(|b| b[3] > 0) {
        let take = left.min(bin[3]);
        for c in 0..3 {
            sum[c] += bin[c] * take / bin[3];
        }
        left -= take;
        if left == 0 {
            break;
        }
    }
    Some(super::pack(sum.map(|v| (v / n) as u8)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Small budgets distribute a finite number of slots; enough slots expose every facial part.
    #[test]
    fn face_slots_stay_ordered_and_inside_the_limit() {
        let guides = [Guide::default(); 2];
        for limit in 1..=64 {
            let layout = Layout::new(&guides, limit);
            let reserved = (0..limit).filter(|&i| layout.outline(i).is_some()).count();
            assert_eq!(reserved, (limit / 2).clamp(1, 18));
            assert!(layout.outline(limit).is_none());
            assert_eq!(layout.detail_start + layout.features.len(), limit);
        }
        let empty = Layout::new(&[], 4);
        assert!((0..4).all(|i| empty.outline(i).is_none() && !empty.needs_padding(i)));
    }
}
