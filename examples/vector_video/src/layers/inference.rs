//! A bounded worker for sender-side inference.
use std::{
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
};

use anyhow::{Context, Result};

#[cfg(feature = "object-layers")]
use ort::session::{builder::SessionBuilder, Session};

/// Opens a publisher model using the platform execution provider when available.
#[cfg(feature = "object-layers")]
pub(super) fn open_model(
    name: &str,
    allow_acceleration: bool,
    load: impl Fn(SessionBuilder) -> Result<Session>,
) -> Result<Session> {
    use std::path::PathBuf;

    #[cfg(target_os = "macos")]
    const LIBRARY: &str = "libonnxruntime.dylib";
    #[cfg(target_os = "windows")]
    const LIBRARY: &str = "onnxruntime.dll";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    const LIBRARY: &str = "libonnxruntime.so";
    let path = std::env::var_os("ORT_DYLIB_PATH")
        .filter(|path| !path.is_empty())
        .map_or_else(|| PathBuf::from(LIBRARY), PathBuf::from);
    ort::init_from(&path)
        .with_context(|| {
            format!(
                "Could not load ONNX Runtime from {}. Set ORT_DYLIB_PATH to the shared library",
                path.display()
            )
        })?
        .with_name("vector-video")
        .commit();

    let builder = || -> Result<_> {
        Ok(Session::builder()?.with_intra_threads(1)?.with_intra_op_spinning(false)?)
    };
    if allow_acceleration {
        #[cfg(target_os = "macos")]
        {
            use ort::ep::coreml::{ComputeUnits, ModelFormat};

            let accelerated = || -> Result<Session> {
                let coreml = ort::ep::CoreML::default()
                    .with_model_format(ModelFormat::MLProgram)
                    .with_compute_units(ComputeUnits::CPUAndGPU)
                    .build()
                    .error_on_failure();
                load(builder()?.with_execution_providers([coreml])?)
            };
            match accelerated() {
                Ok(session) => {
                    eprintln!("{name} using Core ML (CPU and GPU)");
                    return Ok(session);
                }
                Err(error) => log::debug!("{name}: Core ML unavailable: {error:#}"),
            }
        }
    }
    let session = load(builder()?)?;
    eprintln!("{name} using ONNX Runtime CPU");
    Ok(session)
}

/// Maps a destination pixel center to its adjacent source samples and interpolation fraction.
#[cfg(feature = "object-layers")]
pub(super) fn axis(pixel: usize, src: usize, dst: usize) -> (usize, usize, f32) {
    let f = (((pixel as f32 + 0.5) * src as f32 / dst as f32) - 0.5).clamp(0., (src - 1) as f32);
    let lo = f as usize;
    (lo, (lo + 1).min(src - 1), f - lo as f32)
}

/// Interpolates a pixel or model output between neighboring samples.
#[cfg(feature = "object-layers")]
pub(super) fn lerp(lo: f32, hi: f32, t: f32) -> f32 {
    lo + (hi - lo) * t
}

/// Owns a thread with at most one observation in flight.
pub(super) struct Worker<Input, Output> {
    requests: Option<SyncSender<Input>>,
    results: Receiver<Output>,
    thread: Option<JoinHandle<()>>,
    busy: bool,
}

impl<Input: Send + 'static, Output: Send + 'static> Worker<Input, Output> {
    /// Starts a detector loop and gives it exclusive access to its inference session.
    pub(super) fn new(
        name: &str,
        mut infer: impl FnMut(Input) -> Output + Send + 'static,
    ) -> Result<Self> {
        let (requests, rx) = mpsc::sync_channel(1);
        let (tx, results) = mpsc::sync_channel(1);
        let thread = thread::Builder::new().name(name.into()).spawn(move || {
            while let Ok(input) = rx.recv() {
                if tx.send(infer(input)).is_err() {
                    break;
                }
            }
        })?;
        Ok(Self { requests: Some(requests), results, thread: Some(thread), busy: false })
    }

    /// Reports whether the previous request is still running or waiting to be collected.
    pub(super) fn busy(&self) -> bool {
        self.busy
    }

    /// Submits one observation after its predecessor has been collected.
    pub(super) fn submit(&mut self, input: Input) -> Result<()> {
        debug_assert!(!self.busy);
        self.requests
            .as_ref()
            .context("Inference worker stopped")?
            .try_send(input)
            .map_err(|_| anyhow::anyhow!("Inference worker did not accept its next frame"))?;
        self.busy = true;
        Ok(())
    }

    /// Collects a completed observation without blocking frame production.
    pub(super) fn poll(&mut self) -> Result<Option<Output>> {
        if !self.busy {
            return Ok(None);
        }
        match self.results.try_recv() {
            Ok(result) => {
                self.busy = false;
                Ok(Some(result))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                self.busy = false;
                anyhow::bail!("Inference worker stopped")
            }
        }
    }
}

impl<Input, Output> Drop for Worker<Input, Output> {
    fn drop(&mut self) {
        self.requests.take();
        // The one outstanding request can deliver its result even if the publisher stops polling.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
