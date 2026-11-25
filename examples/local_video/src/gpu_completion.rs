//! Drive GPU completion callbacks independently of the next video frame or UI repaint.
use eframe::wgpu;
use log::warn;
use parking_lot::{Condvar, Mutex};
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};

const GPU_POLL_TIMEOUT: Duration = Duration::from_millis(5);
const GPU_POLL_QUEUE_EMPTY_RETRY_DELAY: Duration = Duration::from_micros(100);

/// Pending submissions and the shutdown flag protected by the poller mutex.
#[derive(Default)]
struct GpuCompletionPollState {
    pending: usize,
    shutdown: bool,
}

/// Wakes the polling thread when pending work or shutdown state changes.
#[derive(Default)]
struct GpuCompletionPollShared {
    state: Mutex<GpuCompletionPollState>,
    wake: Condvar,
}

impl GpuCompletionPollShared {
    /// Counts a submission and wakes the thread that services its callback.
    fn begin_probe(self: &Arc<Self>) -> GpuCompletionProbe {
        self.state.lock().pending += 1;
        self.wake.notify_one();
        GpuCompletionProbe { shared: Some(self.clone()) }
    }

    /// Wakes an idle thread so its owner can join it.
    fn request_shutdown(&self) {
        self.state.lock().shutdown = true;
        self.wake.notify_one();
    }
}

/// Keeps the device polling until this submission finishes.
pub(crate) struct GpuCompletionProbe {
    shared: Option<Arc<GpuCompletionPollShared>>,
}

impl GpuCompletionProbe {
    /// Creates a guard when the poll thread could not start.
    fn inactive() -> Self {
        Self { shared: None }
    }
}

impl Drop for GpuCompletionProbe {
    fn drop(&mut self) {
        let Some(shared) = self.shared.take() else {
            return;
        };

        let mut state = shared.state.lock();
        state.pending = state.pending.saturating_sub(1);
        if state.pending == 0 {
            shared.wake.notify_one();
        }
    }
}

/// Owns the idle-aware GPU polling thread for a desktop viewer.
pub(crate) struct GpuCompletionPoller {
    shared: Arc<GpuCompletionPollShared>,
    worker: Option<JoinHandle<()>>,
}

impl GpuCompletionPoller {
    /// Starts polling for completed submissions on this device.
    pub(crate) fn new(device: wgpu::Device) -> Self {
        let shared = Arc::new(GpuCompletionPollShared::default());
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("video-gpu-completion".to_string())
            .spawn(move || poll_gpu_completions(device, worker_shared))
            .map_err(|err| warn!("Unable to start GPU-completion polling thread: {err}"))
            .ok();
        Self { shared, worker }
    }

    /// Wakes the poller until the returned guard is dropped.
    pub(crate) fn begin_probe(&self) -> GpuCompletionProbe {
        if self.worker.is_some() {
            self.shared.begin_probe()
        } else {
            GpuCompletionProbe::inactive()
        }
    }
}

impl Drop for GpuCompletionPoller {
    fn drop(&mut self) {
        self.shared.request_shutdown();
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                warn!("GPU-completion polling thread panicked during shutdown");
            }
        }
    }
}

/// Polls only while completion guards are outstanding and checks shutdown between waits.
fn poll_gpu_completions(device: wgpu::Device, shared: Arc<GpuCompletionPollShared>) {
    let mut poll_error_logged = false;
    loop {
        {
            let mut state = shared.state.lock();
            while state.pending == 0 && !state.shutdown {
                shared.wake.wait(&mut state);
            }
            if state.shutdown {
                return;
            }
        }

        let result = device
            .poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(GPU_POLL_TIMEOUT) });
        match result {
            Ok(status) if status.is_queue_empty() => {
                // The paint callback runs just before eframe submits. If the worker wins
                // that race, briefly back off and wait for the containing submission.
                thread::sleep(GPU_POLL_QUEUE_EMPTY_RETRY_DELAY);
            }
            Ok(_) | Err(wgpu::PollError::Timeout) => {}
            Err(err) => {
                if !poll_error_logged {
                    warn!("Unable to poll GPU completion: {err}");
                    poll_error_logged = true;
                }
                thread::sleep(GPU_POLL_TIMEOUT);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Dropping all probes makes the thread idle until more work or shutdown.
    #[test]
    fn gpu_completion_poll_state_tracks_probes_and_shutdown() {
        let shared = Arc::new(GpuCompletionPollShared::default());
        let first = shared.begin_probe();
        let second = shared.begin_probe();
        assert_eq!(shared.state.lock().pending, 2);

        drop(first);
        assert_eq!(shared.state.lock().pending, 1);
        drop(second);
        assert_eq!(shared.state.lock().pending, 0);

        shared.request_shutdown();
        assert!(shared.state.lock().shutdown);
    }
}
