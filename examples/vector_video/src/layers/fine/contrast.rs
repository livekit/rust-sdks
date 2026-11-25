//! Recovers source contrast along the visible boundaries of small polygons.

use std::collections::HashMap;

use super::{luma, pack, unpack, Color, Scene, DETAIL_PLANE};

/// Measures how consistently the source changes color across the same two layers.
#[derive(Default)]
struct Evidence {
    sum: i32,
    magnitude: i32,
    samples: i32,
}

/// Strengthens underrepresented boundaries by adjusting existing RGB565 fills.
pub(super) fn refit(scene: &mut Scene, source: &[Color], owners: &[u16]) {
    const MIN_AREA: i32 = 8;
    const MAX_AREA: i32 = 800;
    const STEP: usize = 4;
    let width = DETAIL_PLANE.width;
    let mut area = vec![0; scene.patches.len() + 1];
    for &owner in owners {
        area[usize::from(owner)] += 1;
    }
    let adjustable = |i: usize| i != 0 && (MIN_AREA..=MAX_AREA).contains(&area[i]);
    if !area.iter().enumerate().any(|(i, _)| adjustable(i)) {
        return;
    }

    let mut boundaries = HashMap::<(usize, usize), Evidence>::new();
    let mut sample = |first: usize, second: usize| {
        let a = usize::from(owners[first]);
        let b = usize::from(owners[second]);
        if a == b || !(adjustable(a) || adjustable(b)) {
            return;
        }
        let difference = i32::from(source[first][0] - source[second][0]);
        if !(7..=135).contains(&difference.abs()) {
            return;
        }
        let (pair, difference) = if a < b { ((a, b), difference) } else { ((b, a), -difference) };
        let evidence = boundaries.entry(pair).or_default();
        evidence.sum += difference;
        evidence.magnitude += difference.abs();
        evidence.samples += 1;
    };
    for row in (0..owners.len()).step_by(width) {
        for i in row..row + width - STEP {
            sample(i, i + STEP);
        }
    }
    for i in 0..owners.len() - width * STEP {
        sample(i, i + width * STEP);
    }

    let originals: Vec<_> = std::iter::once(scene.background)
        .chain(scene.patches.iter().map(|patch| patch.color))
        .map(unpack)
        .collect();
    let base: Vec<_> = originals.iter().copied().map(luma).collect();
    let mut neighbors = vec![Vec::new(); area.len()];
    for ((a, b), edge) in boundaries {
        if edge.samples < 4 || edge.sum.abs() * 5 < 3 * edge.magnitude {
            continue;
        }
        let current = base[a] - base[b];
        let target = edge.sum / edge.samples;
        let direction = target.signum();
        if target.abs() < 6 || current * direction < 0 || target.abs() <= current.abs() + 4 {
            continue;
        }
        let desired = target.clamp(current - 40, current + 40);
        let weight = 4 * edge.samples;
        neighbors[a].push((b, weight, desired));
        neighbors[b].push((a, weight, -desired));
    }

    let mut visible = base.clone();
    for _ in 0..2 {
        for (index, patch) in (1..).zip(&mut scene.patches) {
            if !adjustable(index) || neighbors[index].is_empty() {
                continue;
            }
            let mut weight = area[index] + 24;
            let mut sum = base[index] * weight;
            for &(other, samples, desired) in &neighbors[index] {
                sum += samples * (visible[other] + desired);
                weight += samples;
            }
            let offset = ((sum + weight / 2) / weight - base[index]).clamp(-16, 16);
            let color = pack(
                originals[index].map(|channel| (i32::from(channel) + offset).clamp(0, 255) as u8),
            );
            let proposed = luma(unpack(color));
            let previous = visible[index];
            let improvement = i64::from(proposed - previous)
                * (2 * i64::from(sum) - i64::from(weight) * i64::from(proposed + previous));
            if improvement > 4 * i64::from(weight) {
                patch.color = color;
                visible[index] = proposed;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::Patch;

    /// A small fill can regain its source contrast while the surrounding surface stays flat.
    #[test]
    fn strengthens_a_dim_boundary_without_moving_it() {
        let background = pack([150; 3]);
        let outline =
            [[40, 40], [50, 40], [50, 50], [40, 50], [40, 50], [40, 50], [40, 50], [40, 50]];
        let dim = pack([140; 3]);
        let mut scene = Scene { background, patches: vec![Patch::polygon(outline, dim)] };
        let owners = scene.owners(DETAIL_PLANE);
        let source: Vec<_> =
            owners.iter().map(|&owner| [if owner == 0 { 150 } else { 104 }, 0, 0]).collect();

        refit(&mut scene, &source, &owners);
        assert!(luma(unpack(scene.patches[0].color)) < luma(unpack(dim)));
        assert_eq!(scene.patches[0].outline, outline);
        assert_eq!(scene.background, background);

        let reversed = pack([164; 3]);
        scene.patches[0].color = reversed;
        refit(&mut scene, &source, &owners);
        assert_eq!(scene.patches[0].color, reversed);
    }
}
