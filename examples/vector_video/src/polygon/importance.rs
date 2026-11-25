//! Multiscale contrast weights for polygon geometry and split selection.
use crate::video::Canvas;

/// Weights for 4×4 analysis tiles, computed exclusively at the sender.
pub(super) struct ImportanceMap {
    weights: Vec<u8>,
    columns: usize,
}

impl ImportanceMap {
    const TILE: usize = 4;

    /// Measures contrast at two scales relative to nearby texture, smoothing across adjacent frames.
    pub(super) fn new(
        image: &[[u8; 3]],
        canvas: Canvas,
        previous: Option<&Self>,
        features: bool,
    ) -> Self {
        let (cols, rows) = (canvas.width.div_ceil(Self::TILE), canvas.height.div_ceil(Self::TILE));
        let tiles = Self::tile_means(image, canvas, cols, rows);
        let gradients = features.then(|| Self::gradients(&tiles, cols, rows));
        let contrast: Vec<_> =
            (0..tiles.len()).map(|i| Self::contrast(&tiles, i, cols, rows)).collect();
        let stride = cols + 1;
        let mut sums = vec![0; (rows + 1) * stride];
        for (y, row) in contrast.chunks_exact(cols).enumerate() {
            let mut sum = 0;
            for (x, &value) in row.iter().enumerate() {
                sum += value;
                sums[(y + 1) * stride + x + 1] = sum + sums[y * stride + x + 1];
            }
        }
        let mut weights = Vec::with_capacity(contrast.len());
        for (i, &signal) in contrast.iter().enumerate() {
            let (x, y) = (i % cols, i / cols);
            let (left, right) = (x.saturating_sub(4), (x + 5).min(cols));
            let (top, bottom) = (y.saturating_sub(4), (y + 5).min(rows));
            let band = |y| sums[y * stride + right] - sums[y * stride + left];
            let surround = band(bottom) - band(top);
            let normalizer = 12 + surround / ((bottom - top) * (right - left)) as u32;
            let strength = gradients.as_ref().map_or(signal, |g| {
                let (edge, corner) = Self::tensor(g, x, y, cols, rows);
                signal / 2 + edge + 2 * corner
            });
            let weight = 1 + (4 * strength.pow(2) / normalizer.pow(2)).min(15) as u8;
            // Retain enough weight in smooth regions for the face colors to remain representative.
            let weight = if features { weight.max(3) } else { weight };
            weights.push(previous.map_or(weight, |p| (2 * weight + p.weights[i] + 1) / 3));
        }
        Self { weights, columns: cols }
    }

    /// Central differences of pooled luma retain line orientation and suppress sensor noise.
    fn gradients(tiles: &[[i32; 3]], cols: usize, rows: usize) -> Vec<[i32; 2]> {
        let luma = |i: usize| {
            let [r, g, b] = tiles[i];
            (r + 2 * g + b) / 4
        };
        (0..tiles.len())
            .map(|i| {
                let (x, y) = (i % cols, i / cols);
                [
                    luma(y * cols + (x + 1).min(cols - 1)) - luma(y * cols + x.saturating_sub(1)),
                    luma((y + 1).min(rows - 1) * cols + x) - luma(y.saturating_sub(1) * cols + x),
                ]
            })
            .collect()
    }

    /// The principal and minor structure-tensor energies measure edges and two-directional detail.
    fn tensor(gradients: &[[i32; 2]], x: usize, y: usize, cols: usize, rows: usize) -> (u32, u32) {
        let (mut xx, mut xy, mut yy, mut n) = (0, 0, 0, 0);
        for row in y.saturating_sub(1)..=(y + 1).min(rows - 1) {
            for &[dx, dy] in
                &gradients[row * cols + x.saturating_sub(1)..=row * cols + (x + 1).min(cols - 1)]
            {
                xx += dx * dx;
                xy += dx * dy;
                yy += dy * dy;
                n += 1;
            }
        }
        let trace = f64::from(xx + yy);
        let gap = f64::from(xx - yy).hypot(2.0 * f64::from(xy));
        let scale = 2.0 * f64::from(n);
        (((trace + gap) / scale).sqrt() as u32, ((trace - gap).max(0.0) / scale).sqrt() as u32)
    }

    /// Pools nearby pixels before measuring edges, reducing the influence of noise and fine texture.
    fn tile_means(image: &[[u8; 3]], canvas: Canvas, cols: usize, rows: usize) -> Vec<[i32; 3]> {
        let mut tiles = vec![[0; 3]; cols * rows];
        for (y, row) in image.chunks_exact(canvas.width).enumerate() {
            for (x, pixel) in row.iter().enumerate() {
                let sum = &mut tiles[(y / Self::TILE) * cols + x / Self::TILE];
                for c in 0..3 {
                    sum[c] += i32::from(pixel[c]);
                }
            }
        }
        for (i, tile) in tiles.iter_mut().enumerate() {
            let width = Self::TILE.min(canvas.width - i % cols * Self::TILE);
            let height = Self::TILE.min(canvas.height - i / cols * Self::TILE);
            *tile = tile.map(|v| v / (width * height) as i32);
        }
        tiles
    }

    /// Finds directional edges and center-surround differences persistent across two spatial scales.
    fn contrast(tiles: &[[i32; 3]], i: usize, cols: usize, rows: usize) -> u32 {
        let (x, y) = ((i % cols) as i32, (i / cols) as i32);
        let at = |dx: i32, dy: i32| {
            let x = (x + dx).clamp(0, cols as i32 - 1) as usize;
            let y = (y + dy).clamp(0, rows as i32 - 1) as usize;
            tiles[y * cols + x]
        };
        let response = |radius| {
            let mut peak = 0;
            for (dx, dy) in [(radius, 0), (0, radius), (radius, radius), (radius, -radius)] {
                let (a, b) = (at(dx, dy), at(-dx, -dy));
                for c in 0..3 {
                    let edge = (a[c] - b[c]).unsigned_abs() / 2;
                    let spot = (tiles[i][c] - (a[c] + b[c]) / 2).unsigned_abs();
                    peak = peak.max(edge).max(spot);
                }
            }
            peak
        };
        let fine = response(1);
        fine + fine.min(response(3))
    }

    /// Looks up the weight at an analysis sample.
    pub(super) fn weight(&self, x: usize, y: usize) -> i32 {
        i32::from(self.weights[(y / Self::TILE) * self.columns + x / Self::TILE])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The minor eigenvalue distinguishes a corner from an equally strong straight edge.
    #[test]
    fn separates_corners_edges_and_flat_areas() {
        let energy = |g: &[[i32; 2]; 9]| ImportanceMap::tensor(g, 1, 1, 3, 3);
        assert_eq!(energy(&[[0, 0]; 9]), (0, 0));
        assert_eq!(energy(&[[80, 0]; 9]), (80, 0));
        assert_eq!(energy(&[[0, 80]; 9]), (80, 0));
        let corner = std::array::from_fn(|i| if i % 2 == 0 { [80, 0] } else { [0, 80] });
        let (edge, minor) = energy(&corner);
        assert!(edge >= minor && minor > 50);
    }
}
