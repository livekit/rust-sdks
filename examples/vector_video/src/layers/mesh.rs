//! Tessellates even-odd polygon fills for the GPU viewer.
use super::{Outline, Point, VERTICES};
use crate::polygon::DisplayTriangle;

/// A polygon edge ordered from top to bottom in wire coordinates.
#[derive(Clone, Copy)]
struct Edge {
    top: [i32; 2],
    bottom: [i32; 2],
}

impl Edge {
    /// Puts both halves of a shared edge in the same order.
    fn new(a: Point, b: Point) -> Self {
        let (top, bottom) = if a[1] <= b[1] { (a, b) } else { (b, a) };
        Self { top: top.map(i32::from), bottom: bottom.map(i32::from) }
    }

    /// Returns the nonnegative height of this edge.
    fn height(self) -> i32 {
        self.bottom[1] - self.top[1]
    }

    /// Interpolates the horizontal coordinate at one row on a nonhorizontal edge.
    fn x(self, y: f64) -> f64 {
        let offset = (y - f64::from(self.top[1])) / f64::from(self.height());
        f64::from(self.top[0]) + offset * f64::from(self.bottom[0] - self.top[0])
    }

    /// Finds a strict crossing inside both edges, excluding shared endpoints.
    fn crossing(self, other: Self) -> Option<f64> {
        let top = self.top[1].max(other.top[1]);
        let bottom = self.bottom[1].min(other.bottom[1]);
        if top >= bottom {
            return None;
        }
        let numerator = |edge: Edge, y| {
            edge.top[0] * edge.height() + (edge.bottom[0] - edge.top[0]) * (y - edge.top[1])
        };
        // Compare the interpolated positions with a common denominator to detect
        // touching and collinear edges exactly before computing the crossing.
        let gap = |y| numerator(self, y) * other.height() - numerator(other, y) * self.height();
        let (start, end) = (gap(top), gap(bottom));
        if start == 0 || end == 0 || start.signum() == end.signum() {
            return None;
        }
        Some(f64::from(top) + f64::from(bottom - top) * f64::from(start) / f64::from(start - end))
    }
}

/// Adds an opaque trapezoid in wire coordinates, omitting triangles with zero area.
pub(super) fn trapezoid([a, b, c, d]: [[f64; 2]; 4], rgb: [u8; 3], out: &mut Vec<DisplayTriangle>) {
    let mut add = |points: [[f64; 2]; 3]| {
        let points = points.map(|p| p.map(|v| (v / 255.0) as f32));
        out.push(DisplayTriangle { points, rgb });
    };
    if a[0] < b[0] {
        add([a, b, c]);
    }
    if d[0] < c[0] {
        add([a, c, d]);
    }
}

/// Splits a polygon at vertex heights and edge crossings so each band has a fixed edge order.
pub(super) fn append(outline: &Outline, rgb: [u8; 3], out: &mut Vec<DisplayTriangle>) {
    let edges: [Edge; VERTICES] =
        std::array::from_fn(|i| Edge::new(outline[i], outline[(i + 1) % VERTICES]));
    // One entry per vertex plus at most one crossing per pair of edges.
    let mut rows = [0.0; VERTICES * (VERTICES + 1) / 2];
    let mut count = VERTICES;
    for (row, &point) in rows.iter_mut().zip(outline) {
        *row = f64::from(point[1]);
    }
    for (i, edge) in edges.iter().enumerate() {
        for &other in &edges[i + 1..] {
            if let Some(y) = edge.crossing(other) {
                rows[count] = y;
                count += 1;
            }
        }
    }
    let rows = &mut rows[..count];
    rows.sort_unstable_by(f64::total_cmp);
    for [top, bottom] in rows.windows(2).map(|pair| [pair[0], pair[1]]) {
        if top == bottom {
            continue;
        }
        let mid = (top + bottom) * 0.5;
        let mut hits = [(0.0, 0); VERTICES];
        let mut count = 0;
        for (i, edge) in edges.iter().enumerate() {
            if f64::from(edge.top[1]) < mid && mid < f64::from(edge.bottom[1]) {
                hits[count] = (edge.x(mid), i);
                count += 1;
            }
        }
        hits[..count].sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        for pair in hits[..count].chunks_exact(2) {
            let [(left_x, left), (right_x, right)] = [pair[0], pair[1]];
            if left_x < right_x {
                let (left, right) = (edges[left], edges[right]);
                trapezoid(
                    [
                        [left.x(top), top],
                        [right.x(top), top],
                        [right.x(bottom), bottom],
                        [left.x(bottom), bottom],
                    ],
                    rgb,
                    out,
                );
            }
        }
    }
}
