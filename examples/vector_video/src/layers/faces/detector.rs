//! CPU inference for BlazeFace proposals and MediaPipe facial landmarks.
use anyhow::{ensure, Context, Result};
use tract_onnx::prelude::*;

use super::{Canvas, Guide};

const DETECTION_SIZE: usize = 128;
const LANDMARK_SIZE: usize = 256;
const MAX_FACES: usize = 4;

/// Maps a unit square to a square crop in the source, including rotation around the eye line.
#[derive(Clone, Copy)]
struct Crop {
    center: [f32; 2],
    side: f32,
    direction: [f32; 2],
}

impl Crop {
    /// Projects coordinates with (0, 0) at the top-left of the model input into source pixels.
    fn project(self, x: f32, y: f32) -> [f32; 2] {
        let [c, s] = self.direction;
        let (dx, dy) = (self.side * (x - 0.5), self.side * (y - 0.5));
        [self.center[0] + c * dx - s * dy, self.center[1] + s * dx + c * dy]
    }

    /// Samples a rotated, black-padded RGB crop directly into normalized planar floats.
    fn tensor(self, rgb: &[u8], size: Canvas, n: usize, gain: f32, bias: f32) -> Result<Tensor> {
        let mut tensor = Tensor::zero::<f32>(&[1, 3, n, n])?;
        let pixels = tensor.as_slice_mut::<f32>()?;
        for y in 0..n {
            for x in 0..n {
                let [sx, sy] = self.project(x as f32 / n as f32, y as f32 / n as f32);
                let mut color = [0.; 3];
                if sx >= 0. && sy >= 0. && sx < size.width as f32 && sy < size.height as f32 {
                    let (l, t) = (sx as usize, sy as usize);
                    let (r, b) = ((l + 1).min(size.width - 1), (t + 1).min(size.height - 1));
                    let (fx, fy) = (sx - l as f32, sy - t as f32);
                    for (px, py, weight) in [
                        (l, t, (1. - fx) * (1. - fy)),
                        (r, t, fx * (1. - fy)),
                        (l, b, (1. - fx) * fy),
                        (r, b, fx * fy),
                    ] {
                        let offset = 3 * (py * size.width + px);
                        for c in 0..3 {
                            color[c] += weight * f32::from(rgb[offset + c]);
                        }
                    }
                }
                for c in 0..3 {
                    pixels[(c * n + y) * n + x] = color[c] * gain + bias;
                }
            }
        }
        Ok(tensor)
    }
}

/// A square face proposal and its eye-line vector, in source pixels.
#[derive(Clone, Copy)]
struct Proposal {
    center: [f32; 2],
    side: f32,
    eyes: [f32; 2],
    score: f32,
}

impl Proposal {
    /// Decodes BlazeFace's two 16×16 or six 8×8 anchors per grid cell.
    fn decode(i: usize, reg: &[f32], logit: f32, view: Crop) -> Option<Self> {
        if logit < 0. || !logit.is_finite() || !reg[..8].iter().all(|v| v.is_finite()) {
            return None;
        }
        let (cell, grid) = if i < 512 { (i / 2, 16) } else { ((i - 512) / 6, 8) };
        let n = DETECTION_SIZE as f32;
        let x = ((cell % grid) as f32 + 0.5) / grid as f32 + reg[0] / n;
        let y = ((cell / grid) as f32 + 0.5) / grid as f32 + reg[1] / n;
        let side = reg[2].max(reg[3]) * view.side / n;
        (side >= 2.).then(|| Self {
            center: view.project(x, y),
            side,
            eyes: [reg[6] - reg[4], reg[7] - reg[5]],
            score: 1. / (1. + (-logit).exp()),
        })
    }

    /// Computes intersection over union of the unexpanded detection squares.
    fn overlap(self, other: Self) -> f32 {
        let intersection: f32 = (0..2)
            .map(|i| {
                let end = (self.center[i] + self.side / 2.).min(other.center[i] + other.side / 2.);
                let start =
                    (self.center[i] - self.side / 2.).max(other.center[i] - other.side / 2.);
                (end - start).max(0.)
            })
            .product();
        intersection / (self.side * self.side + other.side * other.side - intersection)
    }
}

/// Merges duplicate anchors by confidence before expanding each crop for the landmark model.
fn face_crops(mut proposals: Vec<Proposal>) -> Vec<Crop> {
    proposals.sort_unstable_by(|a, b| a.score.total_cmp(&b.score));
    let mut crops = Vec::new();
    while let Some(best) = proposals.pop() {
        let mut sum = [0.; 5];
        let mut weight = 0.;
        let mut add = |p: Proposal| {
            for (dst, value) in
                sum.iter_mut().zip([p.center[0], p.center[1], p.side, p.eyes[0], p.eyes[1]])
            {
                *dst += value * p.score;
            }
            weight += p.score;
        };
        add(best);
        proposals.retain(|&p| {
            let duplicate = best.overlap(p) > 0.3;
            if duplicate {
                add(p);
            }
            !duplicate
        });
        let eye_distance = sum[3].hypot(sum[4]);
        crops.push(Crop {
            center: [sum[0] / weight, sum[1] / weight],
            side: 1.5 * sum[2] / weight,
            direction: if eye_distance > 1e-5 {
                [sum[3] / eye_distance, sum[4] / eye_distance]
            } else {
                [1., 0.]
            },
        });
        if crops.len() == MAX_FACES {
            break;
        }
    }
    crops
}

/// Two reusable plans with weights embedded in the publisher for offline operation.
pub(super) struct Detector {
    boxes: TypedRunnableModel<TypedModel>,
    landmarks: TypedRunnableModel<TypedModel>,
}

impl Detector {
    /// Loads and optimizes the batch-one ONNX graphs for the local CPU.
    pub(super) fn new() -> Result<Self> {
        let load = |mut data: &[u8]| -> Result<_> {
            tract_onnx::onnx().model_for_read(&mut data)?.into_optimized()?.into_runnable()
        };
        Ok(Self {
            boxes: load(include_bytes!("../../../models/blazeface.onnx"))
                .context("Loading BlazeFace")?,
            landmarks: load(include_bytes!("../../../models/face-landmarker.onnx"))
                .context("Loading face landmarker")?,
        })
    }

    /// Finds up to four faces and emits quantized outlines in picture coordinates.
    pub(super) fn detect(&self, rgb: &[u8], size: Canvas) -> Result<Vec<Guide>> {
        ensure!(
            size.width != 0
                && size.height != 0
                && size.width.checked_mul(size.height).and_then(|n| n.checked_mul(3))
                    == Some(rgb.len()),
            "Invalid RGB face detector input"
        );
        let view = Crop {
            center: [size.width as f32 / 2., size.height as f32 / 2.],
            side: size.width.max(size.height) as f32,
            direction: [1., 0.],
        };
        let tensor = view.tensor(rgb, size, DETECTION_SIZE, 1. / 127.5, -1.)?;
        let output = self.boxes.run(tvec!(tensor.into()))?;
        let boxes = output[0].as_slice::<f32>()?;
        let scores = output[1].as_slice::<f32>()?;
        ensure!(boxes.len() == 896 * 16 && scores.len() == 896, "Unexpected BlazeFace output");
        let proposals = boxes
            .chunks_exact(16)
            .zip(scores)
            .enumerate()
            .filter_map(|(i, (reg, &logit))| Proposal::decode(i, reg, logit, view))
            .collect();
        let mut guides = Vec::new();
        for crop in face_crops(proposals) {
            let tensor = crop.tensor(rgb, size, LANDMARK_SIZE, 1. / 255., 0.)?;
            let output = self.landmarks.run(tvec!(tensor.into()))?;
            let xyz = output[0].as_slice::<f32>()?;
            let presence = output[1].as_slice::<f32>()?;
            ensure!(
                xyz.len() == 478 * 3 && presence.len() == 1,
                "Unexpected face landmarker output"
            );
            if presence[0] >= 0. && xyz.iter().all(|v| v.is_finite()) {
                guides.push(guide(xyz, crop, size));
            }
        }
        guides.sort_by_key(|g| g.bounds()[0]);
        Ok(guides)
    }
}

/// Selects ordered contours from MediaPipe's 478-point topology, with four-point irises.
fn guide(xyz: &[f32], crop: Crop, size: Canvas) -> Guide {
    const PARTS: [[usize; 8]; 9] = [
        [10, 284, 454, 365, 152, 136, 234, 54],
        [168, 351, 278, 294, 2, 64, 48, 122],
        [61, 40, 0, 270, 291, 321, 17, 91],
        [33, 161, 159, 157, 133, 153, 145, 163],
        [362, 384, 386, 388, 263, 390, 374, 380],
        [70, 63, 105, 107, 55, 52, 53, 46],
        [336, 296, 334, 300, 276, 283, 282, 285],
        [469, 470, 471, 472, 472, 472, 472, 472],
        [474, 475, 476, 477, 477, 477, 477, 477],
    ];
    Guide {
        parts: PARTS.map(|indices| {
            indices.map(|i| {
                let [x, y] = crop.project(
                    xyz[3 * i] / LANDMARK_SIZE as f32,
                    xyz[3 * i + 1] / LANDMARK_SIZE as f32,
                );
                [
                    (255. * x / size.width as f32).round() as u8,
                    (255. * y / size.height as f32).round() as u8,
                ]
            })
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises embedded weights, landscape padding, both face crops and the no-face path.
    #[test]
    fn detects_two_sets_of_features_and_rejects_a_blank_frame() -> Result<()> {
        let detector = Detector::new()?;
        let frame = include_bytes!("../../../tests/fixtures/head-turn-160x90.rgb");
        let size = Canvas { width: 160, height: 90 };
        let faces = detector.detect(frame, size)?;
        assert_eq!(faces.len(), 2);
        for (face, center) in faces.iter().zip([60, 197]) {
            let [left, right] = [face.parts[3][0], face.parts[4][0]];
            assert!(left[0] < right[0] && left[0].abs_diff(center) < 24);
            assert!(face.parts[0][0][1] < left[1] && face.parts[2][0][1] > left[1]);
            for ring in face.parts {
                assert!(ring.iter().any(|&p| p != ring[0]));
            }
        }
        assert!(detector.detect(&vec![0; frame.len()], size)?.is_empty());
        assert!(detector.detect(&frame[..12], size).is_err());
        Ok(())
    }
}
