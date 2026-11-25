//! Finds subjects inside a rectangular inset when the full-frame mask covers the whole inset.
use super::{
    super::{fit::sample_grid, regions, Canvas, DETAIL_PLANE, H, W},
    vision::Vision,
    Silhouette,
};

/// Bounds of an inset on the coarse fitting grid, with exclusive right and bottom edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Inset {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

impl Inset {
    /// Accepts compact rectangular masks with visible borders on at least three sides.
    fn detect(mask: &[bool], detail: &[[u8; 3]]) -> Option<Self> {
        if mask.len() != W * H || detail.len() != DETAIL_PLANE.width * DETAIL_PLANE.height {
            return None;
        }
        let mut bounds = Self { left: W, top: H, right: 0, bottom: 0 };
        let mut area = 0;
        for (i, &inside) in mask.iter().enumerate() {
            if inside {
                let (x, y) = (i % W, i / W);
                bounds.left = bounds.left.min(x);
                bounds.top = bounds.top.min(y);
                bounds.right = bounds.right.max(x + 1);
                bounds.bottom = bounds.bottom.max(y + 1);
                area += 1;
            }
        }
        if area * 100 < W * H * 8
            || bounds.left < 2
            || bounds.right > W - 2
            || bounds.top < 2
            || bounds.bottom > H - 2
        {
            return None;
        }
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        if width < 30 || height < 18 || area * 10 < 9 * width * height {
            return None;
        }
        bounds.visible_border(detail).then_some(bounds)
    }

    /// Measures RGB differences across the four sides at the fine fitting resolution.
    fn visible_border(self, detail: &[[u8; 3]]) -> bool {
        let (l, t, r, b) = (2 * self.left, 2 * self.top, 2 * self.right, 2 * self.bottom);
        let pixel = |x, y| detail[y * DETAIL_PLANE.width + x];
        let difference = |a: [u8; 3], b: [u8; 3]| -> usize {
            a.into_iter().zip(b).map(|(a, b)| usize::from(a.abs_diff(b))).sum()
        };
        let row = |a, b| (l..r).map(|x| difference(pixel(x, a), pixel(x, b))).sum::<usize>();
        let column = |inside, outside| {
            (t..b).map(|y| difference(pixel(inside, y), pixel(outside, y))).sum::<usize>()
        };
        let horizontal = 3 * 12 * (r - l);
        let vertical = 3 * 12 * (b - t);
        [
            row(t, t - 2) > horizontal,
            row(b - 1, b + 1) > horizontal,
            column(l, l - 2) > vertical,
            column(r - 1, r + 1) > vertical,
        ]
        .into_iter()
        .filter(|&visible| visible)
        .count()
            >= 3
    }

    /// Samples only the inset at the usual Vision input resolution.
    fn sample(self, detail: &[[u8; 3]]) -> Vec<[u8; 3]> {
        let size =
            Canvas { width: 2 * (self.right - self.left), height: 2 * (self.bottom - self.top) };
        sample_grid(size, DETAIL_PLANE, |x, y| {
            detail[(2 * self.top + y) * DETAIL_PLANE.width + 2 * self.left + x]
        })
    }

    /// Maps a crop-local foreground mask into the frame when it adds smaller subjects.
    fn project(self, crop: &[bool]) -> Vec<bool> {
        let occupied = crop.iter().filter(|&&inside| inside).count() * 100;
        if crop.len() != W * H || occupied <= 3 * crop.len() || occupied >= 82 * crop.len() {
            return Vec::new();
        }
        let width = self.right - self.left;
        let height = self.bottom - self.top;
        let mut projected = vec![false; W * H];
        for y in self.top..self.bottom {
            let row = (y - self.top) * H / height * W;
            for x in self.left..self.right {
                projected[y * W + x] = crop[row + (x - self.left) * W / width];
            }
        }
        projected
    }
}

/// Adds up to two inset silhouettes using the same class-agnostic model as the full frame.
pub(super) fn silhouettes(
    model: &mut Vision,
    mask: &[bool],
    detail: &[[u8; 3]],
) -> Vec<Silhouette> {
    let Some(inset) = Inset::detect(mask, detail) else { return Vec::new() };
    match model.detect(&[], &inset.sample(detail)) {
        Ok(crop) => {
            let children = regions::silhouettes(&inset.project(&crop), true);
            log::debug!("Object segmentation found {} inset subjects", children.len());
            children
        }
        Err(error) => {
            log::debug!("Inset segmentation failed: {error:#}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rejects a rectangular mask when the source has no corresponding visible inset.
    #[test]
    fn requires_visible_borders() {
        let inset = Inset { left: 20, top: 16, right: 100, bottom: 62 };
        let mut mask = vec![false; W * H];
        for y in inset.top..inset.bottom {
            mask[y * W + inset.left..y * W + inset.right].fill(true);
        }
        let mut pixels = vec![[20; 3]; DETAIL_PLANE.width * DETAIL_PLANE.height];
        assert_eq!(Inset::detect(&mask, &pixels), None);
        for y in 2 * inset.top..2 * inset.bottom {
            let row = y * DETAIL_PLANE.width;
            pixels[row + 2 * inset.left..row + 2 * inset.right].fill([160; 3]);
        }
        assert_eq!(Inset::detect(&mask, &pixels), Some(inset));
        assert!(inset.sample(&pixels).iter().all(|&pixel| pixel == [160; 3]));
    }

    /// Keeps projected subjects inside the original inset and rejects complete inset masks.
    #[test]
    fn maps_partial_masks_into_the_inset() {
        let inset = Inset { left: 20, top: 16, right: 100, bottom: 62 };
        assert!(inset.project(&vec![true; W * H]).is_empty());
        let mut crop = vec![false; W * H];
        for row in crop.chunks_exact_mut(W).take(H / 2) {
            row[W / 2..].fill(true);
        }
        let mapped = inset.project(&crop);
        for (i, &inside) in mapped.iter().enumerate() {
            assert_eq!(inside, (60..100).contains(&(i % W)) && (16..39).contains(&(i / W)));
        }
    }
}
