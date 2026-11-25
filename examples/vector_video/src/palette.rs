//! Shared RGB565 palette fitting for triangle meshes.
use crate::polygon::{pack, unpack};

/// Squared RGB distance used for assigning colors to palette entries.
pub(crate) fn distance(a: [u8; 3], b: [u8; 3]) -> u32 {
    (0..3).map(|c| u32::from(a[c].abs_diff(b[c])).pow(2)).sum()
}

/// Fits the active colors and repeats the first entry in the unused slots of the wire palette.
pub(crate) fn fit(
    samples: &[[u8; 3]],
    previous: Option<[u16; 16]>,
    stabilize: bool,
    active: usize,
) -> ([[u8; 3]; 16], Vec<u8>) {
    let mut palette = if let Some(palette) = previous {
        palette.map(unpack)
    } else {
        let mut seeds = [samples[0]; 16];
        let mut errors = vec![u32::MAX; samples.len()];
        for k in 1..active {
            for (&rgb, e) in samples.iter().zip(&mut errors) {
                *e = (*e).min(distance(rgb, seeds[k - 1]));
            }
            let i = errors.iter().enumerate().max_by_key(|(_, e)| *e).map_or(0, |(i, _)| i);
            seeds[k] = samples[i];
        }
        seeds
    };
    let prior = palette;
    let mut indices = vec![0; samples.len()];
    let mut errors = vec![0; samples.len()];
    for iteration in 0..4 {
        let mut sums = [[0u32; 3]; 16];
        let mut counts = [0u32; 16];
        for (i, &sample) in samples.iter().enumerate() {
            let (k, error) = palette[..active]
                .iter()
                .enumerate()
                .map(|(k, &p)| (k, distance(sample, p)))
                .min_by_key(|&(_, e)| e)
                .expect("nonempty palette");
            indices[i] = k as u8;
            errors[i] = error;
            counts[k] += 1;
            for (c, sum) in sums[k].iter_mut().enumerate() {
                *sum += u32::from(sample[c]);
            }
        }
        if iteration == 3 {
            break;
        }
        for k in 0..active {
            if counts[k] > 0 {
                palette[k] = unpack(pack(sums[k].map(|v| ((v + counts[k] / 2) / counts[k]) as u8)));
            } else if let Some((i, error)) = errors.iter_mut().enumerate().max_by_key(|(_, e)| **e)
            {
                palette[k] = samples[i];
                *error = 0;
            }
            if iteration == 2
                && stabilize
                && previous.is_some()
                && distance(prior[k], palette[k]) <= 32
            {
                palette[k] = prior[k];
            }
        }
    }
    let first = palette[0];
    palette[active..].fill(first);
    (palette, indices)
}
