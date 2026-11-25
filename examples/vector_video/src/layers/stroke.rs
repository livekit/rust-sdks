//! Fits thin contours as open paths and expands their centerlines for rendering.

use anyhow::{ensure, Result};

use super::{Canvas, Outline, Point, RasterPoint, PLANE, RECORD};

const MAX_WIDTH: u8 = 16;

/// A constant-width path with an optional interior vertex, stored in wire coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Stroke {
    points: [Point; 3],
    width: u8,
}

/// Recognizes the reserved prefix in a stroke-enabled layered frame.
pub(crate) fn is_stroke_record(record: &[u8; RECORD]) -> bool {
    record[4..8] == [0; 4] && record[8] != 0 && record[9] == 0
}

impl Stroke {
    /// Fits the centerline and width of a long, uniformly thin contour.
    pub(super) fn fit(outline: &Outline) -> Option<Self> {
        let points = outline.map(project);
        let mut length_sq = 0.0;
        let mut start = points[0];
        let mut end = start;
        for (i, &a) in points.iter().enumerate() {
            for &b in &points[i + 1..] {
                let [dx, dy] = [b[0] - a[0], b[1] - a[1]];
                let distance = dx * dx + dy * dy;
                if distance > length_sq {
                    (length_sq, start, end) = (distance, a, b);
                }
            }
        }
        if length_sq < 49.0 {
            return None;
        }
        let length = length_sq.sqrt();
        let relative = points.map(|[x, y]| [x - start[0], y - start[1]]);
        let area = relative
            .iter()
            .zip(relative.iter().cycle().skip(1))
            .map(|(p, q)| p[0] * q[1] - p[1] * q[0])
            .sum::<f32>()
            .abs()
            * 0.5;
        // Broad contours cannot meet the width and coverage limits even at maximum thickness.
        let max_width = 5.5_f32.min(length / 4.0);
        if area / (length * max_width) > 1.25 {
            return None;
        }
        let axis = [(end[0] - start[0]) / length, (end[1] - start[1]) / length];
        let local = relative.map(|[x, y]| [x * axis[0] + y * axis[1], y * axis[0] - x * axis[1]]);
        let (a, w0) = cross_section(&local, 0.15 * length)?;
        let (b, w1) = cross_section(&local, 0.50 * length)?;
        let (c, w2) = cross_section(&local, 0.85 * length)?;
        let width = (w0 + w1 + w2) / 3.0;
        let bend = (b - (a + c) * 0.5).abs();
        if width > max_width || w0.max(w1).max(w2) > 2.1 * w0.min(w1).min(w2) || bend > length / 7.0
        {
            return None;
        }
        if !(0.72..=1.25).contains(&(area / (length * width))) {
            return None;
        }
        let quantized = |along: f32, across: f32| {
            let x = start[0] + along * axis[0] - across * axis[1];
            let y = start[1] + along * axis[1] + across * axis[0];
            [
                (x * 255.0 / PLANE.width as f32).round() as u8,
                (y * 255.0 / PLANE.height as f32).round() as u8,
            ]
        };
        let mut a = quantized(0.0, a);
        let mut c = quantized(length, c);
        let b = quantized(length * 0.5, b);
        if c < a {
            std::mem::swap(&mut a, &mut c);
        }
        if a == c {
            return None;
        }
        let middle = if bend >= 0.55 && b != a && b != c { b } else { c };
        Some(Self {
            points: [a, middle, c],
            width: ((width * 255.0 / PLANE.height as f32).round() as u8).clamp(1, MAX_WIDTH),
        })
    }

    /// Writes two or three centerline points into the space of one polygon record.
    pub(super) fn write(self, out: &mut Vec<u8>) {
        let [a, b, c] = self.points;
        out.extend(a);
        out.extend([0; 4]);
        out.extend([self.width, 0]);
        out.extend([b[0].wrapping_sub(a[0]), b[1].wrapping_sub(a[1])]);
        out.extend([c[0].wrapping_sub(b[0]), c[1].wrapping_sub(b[1])]);
        out.extend([0; 4]);
    }

    /// Checks the reserved fields and decodes the open centerline.
    pub(super) fn read(record: &[u8; RECORD]) -> Result<Self> {
        ensure!(record[8] <= MAX_WIDTH && record[14..18] == [0; 4], "Invalid stroke record");
        let a = [record[2], record[3]];
        let b = [a[0].wrapping_add(record[10]), a[1].wrapping_add(record[11])];
        let c = [b[0].wrapping_add(record[12]), b[1].wrapping_add(record[13])];
        ensure!(a != b && a != c, "Invalid stroke path");
        Ok(Self { points: [a, b, c], width: record[8] })
    }

    /// Expands the same clipped strip for CPU rasterization and GPU playback.
    pub(super) fn quads(self, canvas: Canvas) -> [Option<[RasterPoint; 4]>; 2] {
        let [a, b, c] = self.points.map(project);
        let normal = |[x, y]: RasterPoint, [u, v]: RasterPoint| {
            let (dx, dy) = (u - x, v - y);
            let length = (dx * dx + dy * dy).sqrt();
            [-dy / length, dx / length]
        };
        let first = normal(a, b);
        let half = f32::from(self.width) * PLANE.height as f32 / 510.0;
        let station = |[x, y]: RasterPoint, [dx, dy]: RasterPoint, scale: f32| {
            [1.0, -1.0].map(|sign| {
                [
                    (x + sign * scale * dx).clamp(0.0, PLANE.width as f32) * canvas.width as f32
                        / PLANE.width as f32,
                    (y + sign * scale * dy).clamp(0.0, PLANE.height as f32) * canvas.height as f32
                        / PLANE.height as f32,
                ]
            })
        };
        let left = station(a, first, half);
        let quad = |[a, b]: [RasterPoint; 2], [c, d]: [RasterPoint; 2]| [b, d, c, a];
        if b == c {
            return [Some(quad(left, station(b, first, half))), None];
        }
        let second = normal(b, c);
        let joint = station(
            b,
            [first[0] + second[0], first[1] + second[1]],
            half / (1.0 + first[0] * second[0] + first[1] * second[1]).max(0.5),
        );
        [Some(quad(left, joint)), Some(quad(joint, station(c, second, half)))]
    }
}

/// Projects wire coordinates into the plane in which stroke widths are fitted.
fn project([x, y]: Point) -> RasterPoint {
    [x as f32 * PLANE.width as f32 / 255.0, y as f32 * PLANE.height as f32 / 255.0]
}

/// Measures a contour's center and width across the dominant axis.
fn cross_section(points: &[RasterPoint], along: f32) -> Option<(f32, f32)> {
    let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
    for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
        if (a[0] > along) != (b[0] > along) {
            let across = a[1] + (b[1] - a[1]) * (along - a[0]) / (b[0] - a[0]);
            low = low.min(across);
            high = high.max(across);
        }
    }
    (high > low).then_some(((low + high) * 0.5, high - low))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fits straight and bent linework while keeping broad regions as closed contours.
    #[test]
    fn recognizes_thin_contours() {
        let straight =
            [[25, 45], [140, 45], [140, 53], [25, 53], [25, 53], [25, 53], [25, 53], [25, 53]];
        let bent =
            [[20, 55], [86, 70], [145, 53], [145, 62], [86, 79], [20, 64], [20, 64], [20, 64]];
        let broad = straight.map(|[x, y]| [x, if y == 53 { 100 } else { y }]);
        assert!(Stroke::fit(&straight).unwrap().quads(PLANE)[1].is_none());
        assert!(Stroke::fit(&bent).unwrap().quads(PLANE)[1].is_some());
        assert!(Stroke::fit(&broad).is_none());
    }
}
