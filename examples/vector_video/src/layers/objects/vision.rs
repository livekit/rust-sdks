//! Safe entry point for the scoped Apple Vision foreground request.
use anyhow::{ensure, Result};

use super::super::{DETAIL_PLANE, PLANE};

unsafe extern "C" {
    fn vector_video_vision_mask(
        rgb: *const u8,
        width: usize,
        height: usize,
        mask: *mut u8,
        mask_width: usize,
        mask_height: usize,
    ) -> i32;
}

/// Reuses the binary mask allocation across Apple Vision observations.
pub(super) struct Vision {
    mask: Vec<u8>,
}

impl Vision {
    /// Warms up the system model before starting the video cadence.
    pub(super) fn new() -> Result<Self> {
        let mut vision = Self { mask: vec![0; PLANE.width * PLANE.height] };
        let warmup = vec![[0; 3]; DETAIL_PLANE.width * DETAIL_PLANE.height];
        vision.detect(&[], &warmup)?;
        eprintln!("Object segmentation using Apple Vision");
        Ok(vision)
    }

    /// Runs the instance mask request and returns its foreground pixels on the fitting grid.
    pub(super) fn detect(&mut self, coarse: &[[u8; 3]], detail: &[[u8; 3]]) -> Result<Vec<bool>> {
        let (pixels, size) =
            if detail.is_empty() { (coarse, PLANE) } else { (detail, DETAIL_PLANE) };
        ensure!(pixels.len() == size.width * size.height, "Invalid Vision input size");
        // SAFETY: Both pointers address fully sized, nonoverlapping slices. The shim uses
        // the buffers synchronously and retains neither pointer across a call.
        let status = unsafe {
            vector_video_vision_mask(
                pixels.as_flattened().as_ptr(),
                size.width,
                size.height,
                self.mask.as_mut_ptr(),
                PLANE.width,
                PLANE.height,
            )
        };
        match status {
            0 => Ok(self.mask.iter().map(|&value| value != 0).collect()),
            1 => anyhow::bail!("Apple Vision foreground segmentation requires macOS 14 or later"),
            2 => anyhow::bail!("Apple Vision could not create an input pixel buffer"),
            3 => anyhow::bail!("Apple Vision foreground segmentation request failed"),
            _ => anyhow::bail!("Apple Vision returned an invalid instance mask"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::inference::Worker;
    use std::sync::mpsc::{channel, TryRecvError};

    /// Runs scoped requests across worker teardown, including rejection and an in-flight shutdown.
    #[test]
    #[ignore = "requires macOS Vision hardware access"]
    fn vision_requests_release_worker_on_shutdown() -> Result<()> {
        let fixture =
            include_bytes!("../../../tests/fixtures/head-turn-160x90.rgb").as_chunks::<3>().0;
        let samples: Vec<_> = (0..DETAIL_PLANE.width * DETAIL_PLANE.height)
            .map(|i| {
                let x = i % DETAIL_PLANE.width * 160 / DETAIL_PLANE.width;
                let y = i / DETAIL_PLANE.width * 90 / DETAIL_PLANE.height;
                fixture[y * 160 + x]
            })
            .collect();
        for _ in 0..3 {
            let mut vision = Vision::new()?;
            assert!(vision.detect(&[], &samples[..1]).is_err());
            let mask = vision.detect(&[], &samples)?;
            assert!(mask.iter().any(|&inside| inside));
            let (sent, lifetime) = channel::<()>();
            let mut worker = Worker::new("vision-teardown-test", move |image: Vec<[u8; 3]>| {
                let _lease = &sent;
                vision.detect(&[], &image)
            })?;
            worker.submit(samples.clone())?;
            drop(worker);
            assert_eq!(lifetime.try_recv(), Err(TryRecvError::Disconnected));
        }
        Ok(())
    }
}
