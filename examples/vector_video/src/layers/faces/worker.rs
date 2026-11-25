//! One in-flight CPU inference per live publisher.
use anyhow::Result;

use super::{Canvas, Guide};
use crate::layers::inference;

/// A source-resolution RGB image and its dimensions.
type Request = (Vec<u8>, Canvas);
/// The input buffer to recycle and the detected facial contours.
type Observation = (Vec<u8>, Result<Vec<Guide>>);

/// Owns one detector thread and recycles its input allocation after each observation.
pub(super) struct Worker {
    detector: inference::Worker<Request, Observation>,
    buffer: Vec<u8>,
    reference: Vec<[u8; 3]>,
    discard: bool,
}

impl Worker {
    /// Starts the detector loop with a single request and result slot.
    pub(super) fn new(
        detect: impl Fn(&[u8], Canvas) -> Result<Vec<Guide>> + Send + 'static,
    ) -> Result<Self> {
        let detector =
            inference::Worker::new("vector-face-landmarks", move |(rgb, size): Request| {
                let guides = detect(&rgb, size);
                (rgb, guides)
            })?;
        Ok(Self { detector, buffer: Vec::new(), reference: Vec::new(), discard: false })
    }

    /// Reports whether the face observation is still outstanding.
    pub(super) fn busy(&self) -> bool {
        self.detector.busy()
    }

    /// Skips the pending observation if its frame preceded a scene cut.
    pub(super) fn invalidate(&mut self) {
        self.discard = self.busy();
    }

    /// Submits the current frame only when the previous observation has been consumed.
    pub(super) fn submit(&mut self, rgb: &[u8], size: Canvas, plane: &[[u8; 3]]) -> Result<()> {
        debug_assert!(!self.busy());
        self.buffer.clear();
        self.buffer.extend_from_slice(rgb);
        self.reference.clear();
        self.reference.extend_from_slice(plane);
        self.detector.submit((std::mem::take(&mut self.buffer), size))
    }

    /// Polls without delaying capture and aligns late observations with the current frame.
    pub(super) fn poll(&mut self, plane: &[[u8; 3]]) -> Result<Option<Vec<Guide>>> {
        let Some((buffer, guides)) = self.detector.poll()? else {
            return Ok(None);
        };
        self.buffer = buffer;
        if std::mem::take(&mut self.discard) {
            return Ok(None);
        }
        let mut guides = guides?;
        if self.reference != plane {
            for guide in &mut guides {
                guide.follow(&self.reference, plane);
            }
        }
        Ok(Some(guides))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread};

    /// A cut invalidates an in-flight observation, and dropping a busy worker joins its detector.
    #[test]
    fn invalidates_pending_results_and_joins_on_drop() {
        let owner = std::sync::Arc::new(());
        let weak = std::sync::Arc::downgrade(&owner);
        let (started, observed) = mpsc::sync_channel(1);
        let (release, resume) = mpsc::sync_channel(1);
        let mut worker = Worker::new(move |_, _| {
            let _keep_alive = &owner;
            started.send(()).unwrap();
            resume.recv().unwrap();
            Ok(vec![Guide::default()])
        })
        .unwrap();
        let size = Canvas { width: 2, height: 2 };
        worker.submit(&[0; 12], size, &[]).unwrap();
        observed.recv().unwrap();
        worker.invalidate();
        assert!(worker.poll(&[]).unwrap().is_none());
        release.send(()).unwrap();
        while worker.busy() {
            assert!(worker.poll(&[]).unwrap().is_none());
            thread::yield_now();
        }
        worker.submit(&[0; 12], size, &[]).unwrap();
        observed.recv().unwrap();
        release.send(()).unwrap();
        drop(worker);
        assert!(weak.upgrade().is_none());
    }
}
