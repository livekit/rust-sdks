//! Groups edge-adjacent low-contrast faces under a shared fill.

use super::{pack, palette, unpack, FaceColors, Grid, Mesh, Region};

const FACE_SPAN: u8 = 20;
const GROUP_SPAN: u8 = 28;
const FILL_TOLERANCE: u8 = 18;
const MAX_IMPORTANCE: i32 = 9;
const FILL_HYSTERESIS: u32 = 64;

/// Source color bounds and sufficient statistics for a connected set of faces.
#[derive(Clone, Copy)]
struct Tone {
    low: [u8; 3],
    high: [u8; 3],
    sum: [i64; 3],
    mass: i64,
}

impl Tone {
    /// Keeps faces with consistently similar child colors and little structural contrast.
    fn from_face(children: &[Region; 4]) -> Option<Self> {
        let count: i32 = children.iter().map(|child| child.count).sum();
        let importance: i32 = children.iter().map(|child| child.importance).sum();
        if importance > count * MAX_IMPORTANCE {
            return None;
        }
        let mut tone = Self { low: [255; 3], high: [0; 3], sum: [0; 3], mass: 0 };
        for child in children {
            let mean = child.mean();
            tone = tone.join(Self {
                low: mean,
                high: mean,
                sum: child.sum,
                mass: i64::from(child.mass),
            });
        }
        tone.bounded(FACE_SPAN).then_some(tone)
    }

    /// Combines adjacent groups without forgetting any member's color range.
    fn join(self, other: Self) -> Self {
        Self {
            low: std::array::from_fn(|i| self.low[i].min(other.low[i])),
            high: std::array::from_fn(|i| self.high[i].max(other.high[i])),
            sum: std::array::from_fn(|i| self.sum[i] + other.sum[i]),
            mass: self.mass + other.mass,
        }
    }

    /// Checks the full color span to prevent a chain of small changes from flattening a gradient.
    fn bounded(self, max_span: u8) -> bool {
        (0..3).all(|i| self.high[i] - self.low[i] <= max_span)
    }

    /// Returns the fitted color of the source samples in the group.
    fn mean(self) -> [u8; 3] {
        self.sum.map(|v| ((v + self.mass / 2) / self.mass) as u8)
    }

    /// Checks the proposed fill against every member's source range.
    fn accepts(self, fill: [u8; 3]) -> bool {
        (0..3).all(|i| {
            self.low[i].abs_diff(fill[i]).max(self.high[i].abs_diff(fill[i])) <= FILL_TOLERANCE
        })
    }

    /// Finds the closest usable fill within the existing mesh color representation.
    fn fill(self, colors: Option<&[[u8; 3]; 16]>, previous: Option<u16>) -> Option<u16> {
        if !self.bounded(GROUP_SPAN) {
            return None;
        }
        let mean = self.mean();
        let rgb = |fill: u16| colors.map_or_else(|| unpack(fill), |p| p[usize::from(fill)]);
        let best = if let Some(colors) = colors {
            colors
                .iter()
                .enumerate()
                .filter(|&(_, &rgb)| self.accepts(rgb))
                .min_by_key(|&(_, &rgb)| palette::distance(mean, rgb))
                .map(|(i, _)| i as u16)?
        } else {
            let word = pack(mean);
            self.accepts(unpack(word)).then_some(word)?
        };
        Some(
            previous
                .filter(|&fill| {
                    let prior = rgb(fill);
                    self.accepts(prior)
                        && palette::distance(mean, prior)
                            <= palette::distance(mean, rgb(best)) + FILL_HYSTERESIS
                })
                .unwrap_or(best),
        )
    }
}

/// Enumerates each base-grid edge shared by two faces exactly once.
fn shared_edges(grid: Grid) -> impl Iterator<Item = (usize, usize)> {
    let (width, height) = grid.dimensions();
    (0..width * height).flat_map(move |i| {
        let (x, y) = (i % width, i / width);
        let right = 2 * i + (x + y) % 2;
        [
            Some((2 * i, 2 * i + 1)),
            (x + 1 < width).then_some((right, right + 2)),
            (y + 1 < height).then_some((2 * i + 1, 2 * (i + width))),
        ]
        .into_iter()
        .flatten()
    })
}

/// Finds a group's representative, shortening the path for later lookups.
fn root(parents: &mut [usize], mut index: usize) -> usize {
    while parents[index] != index {
        parents[index] = parents[parents[index]];
        index = parents[index];
    }
    index
}

/// Retains nearby shared fills across frames and caches the last body for repeated input.
#[derive(Default)]
pub(super) struct Merger {
    previous: Vec<Option<u16>>,
    body: Vec<u8>,
}

impl Merger {
    /// Writes the same merged mesh when source motion is below the encoder's repeat threshold.
    pub(super) fn repeat(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.body);
    }

    /// Reuses stable joins before merging new neighbors with similar colors.
    pub(super) fn write(
        &mut self,
        mesh: &Mesh,
        samples: &[[Region; 4]],
        tracked: bool,
        out: &mut Vec<u8>,
    ) {
        self.body.clear();
        if !tracked {
            self.previous.clear();
        }
        merge(mesh, samples, &mut self.previous, &mut self.body);
        self.repeat(out);
    }
}

/// Coalesces nearby tones while keeping the original offsets and unmerged detail untouched.
fn merge(mesh: &Mesh, samples: &[[Region; 4]], previous: &mut Vec<Option<u16>>, out: &mut Vec<u8>) {
    let mut groups: Vec<_> = samples.iter().map(Tone::from_face).collect();
    previous.resize(groups.len(), None);
    let mut edges: Vec<_> = shared_edges(mesh.grid)
        .filter_map(|(i, j)| {
            let retained = previous[i].is_some() && previous[i] == previous[j];
            Some((!retained, palette::distance(groups[i]?.mean(), groups[j]?.mean()), i, j))
        })
        .collect();
    edges.sort_unstable();
    let palette = match &mesh.colors {
        FaceColors::Direct(_) => None,
        FaceColors::Indexed { palette, .. } => Some(palette.map(unpack)),
    };
    let mut parents: Vec<_> = (0..groups.len()).collect();
    let mut fills = vec![None; groups.len()];
    for (_, _, i, j) in edges {
        let (i, j) = (root(&mut parents, i), root(&mut parents, j));
        if i == j {
            continue;
        }
        let (Some(a), Some(b)) = (groups[i], groups[j]) else { continue };
        let joined = a.join(b);
        let a_fill = fills[i].or(previous[i]);
        let b_fill = fills[j].or(previous[j]);
        let preferred = if a_fill == b_fill || a.mass >= b.mass { a_fill } else { b_fill };
        if let Some(fill) = joined.fill(palette.as_ref(), preferred) {
            let (keep, alias) = if a.mass >= b.mass { (i, j) } else { (j, i) };
            parents[alias] = keep;
            groups[keep] = Some(joined);
            fills[keep] = Some(fill);
        }
    }
    previous.iter_mut().enumerate().for_each(|(i, fill)| *fill = fills[root(&mut parents, i)]);
    if previous.iter().all(Option::is_none) {
        mesh.write(out);
        return;
    }
    let mut colors = mesh.colors.clone();
    let mut detail = mesh.detail.clone();
    for (i, &fill) in previous.iter().enumerate() {
        if let Some(fill) = fill {
            match &mut colors {
                FaceColors::Direct(words) => words[i] = fill,
                FaceColors::Indexed { indices, .. } => indices[i] = fill as u8,
            }
            if let Some(split) = detail.get_mut(i) {
                *split = None;
            }
        }
    }
    if detail.iter().all(Option::is_none) {
        detail.clear();
    }
    mesh.write_fills(&colors, &detail, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::Canvas;

    /// The checkerboard diagonals connect all shared edges and exclude vertex-only contacts.
    #[test]
    fn adjacency_matches_mesh_connectivity() {
        let grid = Grid::Preset(3);
        let faces: Vec<_> = grid.faces().collect();
        let expected: std::collections::HashSet<_> = (0..faces.len())
            .flat_map(|a| ((a + 1)..faces.len()).map(move |b| (a, b)))
            .filter(|&(a, b)| faces[a].iter().filter(|v| faces[b].contains(v)).count() == 2)
            .collect();
        let edges: Vec<_> = shared_edges(grid).collect();
        assert_eq!(edges.len(), expected.len());
        assert_eq!(edges.into_iter().collect::<std::collections::HashSet<_>>(), expected);
    }

    /// Merges a weak split into an adjacent fill while keeping a nearby sharp split intact.
    #[test]
    fn preserves_sharp_detail_and_wire_geometry() {
        let grid = Grid::Preset(3);
        let palette =
            std::array::from_fn(|i| pack([if i < 4 { [98, 112, 133, 204][i] } else { 28 }; 3]));
        let count = grid.face_count();
        let mut detail = vec![None; count];
        detail[0] = Some([0, 1, 0, 1]);
        detail[2] = Some([2, 3, 2, 3]);
        let mesh = Mesh {
            grid,
            offsets: vec![[0; 2]; grid.vertices()],
            colors: FaceColors::Indexed {
                palette,
                indices: (0..count).map(|i| i.min(4) as u8).collect(),
            },
            detail,
        };
        let region =
            |v: u8| Region { sum: [i64::from(v) * 8; 3], count: 8, importance: 24, mass: 8 };
        let samples: Vec<_> = (0..count)
            .map(|i| match i {
                0 => [104, 102, 104, 107].map(region),
                1 => [117, 119, 120, 118].map(region),
                2 => [130, 197, 143, 214].map(region),
                _ => [28, 29, 27, 28].map(region),
            })
            .collect();
        let mut out = Vec::new();
        Merger::default().write(&mesh, &samples, false, &mut out);
        let decoded = Mesh::decode(&out, Canvas::DEFAULT).unwrap();
        assert_eq!(decoded.offsets, mesh.offsets);
        assert_eq!(decoded.colors.at(0), decoded.colors.at(1));
        assert!(decoded.detail[0].is_none());
        assert_eq!(decoded.detail[2], mesh.detail[2]);
        assert_eq!(decoded.len() + 3, mesh.len());
    }

    /// A gradual chain cannot erase a larger tonal transition across the scene.
    #[test]
    fn bounds_the_entire_group_color_range() {
        let tone = |v: u8| Tone { low: [v; 3], high: [v; 3], sum: [i64::from(v); 3], mass: 1 };
        assert!(tone(96).join(tone(108)).fill(None, None).is_some());
        assert!(tone(108).join(tone(122)).fill(None, None).is_some());
        assert!(tone(96).join(tone(108)).join(tone(122)).fill(None, None).is_some());
        assert!(tone(96)
            .join(tone(108))
            .join(tone(122))
            .join(tone(134))
            .fill(None, None)
            .is_none());
    }
}
