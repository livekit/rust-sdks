// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    error::Error,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use dashmap::{mapref::one::MappedRef, DashMap};
use downcast_rs::{impl_downcast, Downcast};
use livekit::webrtc::{
    native::apm::AudioProcessingModule, native::audio_resampler::AudioResampler, prelude::*,
};
use parking_lot::{deadlock, Mutex};
use tokio::{sync::oneshot, task::JoinHandle};

use crate::{proto, proto::FfiEvent, FfiError, FfiHandleId, FfiResult, INVALID_HANDLE};

pub mod audio_plugin;
pub mod audio_source;
pub mod audio_stream;
pub mod colorcvt;
pub mod data_stream;
pub mod logger;
pub mod participant;
pub mod requests;
pub mod resampler;
pub mod room;
mod utils;
pub mod video_source;
pub mod video_stream;

//#[cfg(test)]
//mod tests;

#[derive(Clone)]
pub struct FfiConfig {
    pub callback_fn: Arc<dyn Fn(FfiEvent) + Send + Sync>,
    pub capture_logs: bool,
    pub sdk: String,
    pub sdk_version: String,
}

/// To make sure we use the right types, only types that implement this trait
/// can be stored inside the FfiServer.
pub trait FfiHandle: Downcast + Send + Sync {}
impl_downcast!(FfiHandle);

#[derive(Clone)]
pub struct FfiDataBuffer {
    pub handle: FfiHandleId,
    pub data: Arc<Vec<u8>>,
}

impl FfiHandle for FfiDataBuffer {}
impl FfiHandle for Arc<Mutex<AudioResampler>> {}
impl FfiHandle for Arc<Mutex<AudioProcessingModule>> {}
impl FfiHandle for Arc<Mutex<resampler::SoxResampler>> {}
impl FfiHandle for AudioFrame<'static> {}
impl FfiHandle for BoxVideoBuffer {}
impl FfiHandle for Box<[u8]> {}
impl FfiHandle for () {}

pub struct FfiServer {
    /// Store all Ffi handles inside an HashMap, if this isn't efficient enough
    /// We can still use Box::into_raw & Box::from_raw in the future (but keep it safe for now)
    ffi_handles: DashMap<FfiHandleId, Box<dyn FfiHandle>>,
    pub async_runtime: tokio::runtime::Runtime,
    /// Dedicated high-priority runtime for audio capture to reduce scheduling delays
    pub audio_runtime: tokio::runtime::Runtime,

    next_id: AtomicU64,
    config: Mutex<Option<FfiConfig>>,
    logger: &'static logger::FfiLogger,
    handle_dropped_txs: DashMap<FfiHandleId, Vec<oneshot::Sender<()>>>,
}

impl Default for FfiServer {
    fn default() -> Self {
        let async_runtime =
            tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();

        // Dedicated single-threaded runtime for audio capture with high priority
        // This ensures audio tasks never compete with other operations
        let audio_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("livekit-audio")
            .on_thread_start(|| {
                log::info!("[AUDIO_THREAD] Starting audio thread initialization...");

                // Set high priority for audio thread to reduce OS scheduling delays
                #[cfg(target_os = "macos")]
                {
                    // Try macOS-specific QoS first
                    if let Err(e) = Self::set_macos_qos_priority() {
                        log::warn!("[AUDIO_THREAD] Failed to set macOS QoS priority: {:?}", e);
                        // Fallback to cross-platform priority
                        Self::set_crossplatform_priority();
                    } else {
                        log::info!("[AUDIO_THREAD] Successfully set macOS QoS to USER_INTERACTIVE");
                    }
                }

                #[cfg(not(target_os = "macos"))]
                {
                    Self::set_crossplatform_priority();
                }

                log::info!("[AUDIO_THREAD] Audio thread initialization complete");
            })
            .enable_all()
            .build()
            .unwrap();

        log::info!("Initialized FFI runtimes: async_runtime + 1 dedicated audio thread");

        let logger = Box::leak(Box::new(logger::FfiLogger::new(async_runtime.handle().clone())));
        log::set_logger(logger).unwrap();
        log::set_max_level(log::LevelFilter::Trace);

        #[cfg(feature = "tracing")]
        console_subscriber::init();

        // Create a background thread which checks for deadlocks every 10s
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(10));
            let deadlocks = deadlock::check_deadlock();
            if deadlocks.is_empty() {
                continue;
            }

            log::error!("{} deadlocks detected", deadlocks.len());
            for (i, threads) in deadlocks.iter().enumerate() {
                log::error!("Deadlock #{}", i);
                for t in threads {
                    log::error!("Thread Id {:#?}: \n{:#?}", t.thread_id(), t.backtrace());
                }
            }
        });

        Self {
            ffi_handles: Default::default(),
            next_id: AtomicU64::new(1), // 0 is invalid
            async_runtime,
            audio_runtime,
            config: Default::default(),
            logger,
            handle_dropped_txs: Default::default(),
        }
    }
}

// Using &'static self inside the implementation, not sure if this is really idiomatic
// It simplifies the code a lot tho. In most cases the server is used until the end of the process
impl FfiServer {
    /// Set macOS-specific QoS to USER_INTERACTIVE (highest user-space priority)
    #[cfg(target_os = "macos")]
    fn set_macos_qos_priority() -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;
            const PTHREAD_PRIORITY_INHERIT: i32 = 0;

            extern "C" {
                fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
            }

            let result =
                pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, PTHREAD_PRIORITY_INHERIT);

            if result == 0 {
                Ok(())
            } else {
                Err(format!("pthread_set_qos_class_self_np failed with code: {}", result).into())
            }
        }
    }

    /// Set cross-platform thread priority (works on macOS, Linux, Windows)
    fn set_crossplatform_priority() {
        use thread_priority::*;

        // Try maximum priority first
        let priority = ThreadPriority::Max;
        if let Err(e) = set_current_thread_priority(priority) {
            log::warn!(
                "[AUDIO_THREAD] Failed to set Max priority: {:?}, trying Crossplatform(80)",
                e
            );

            // Fallback to priority level 80 (high but not max)
            if let Err(e) =
                set_current_thread_priority(ThreadPriority::Crossplatform(80.try_into().unwrap()))
            {
                log::error!("[AUDIO_THREAD] Failed to set any thread priority: {:?}", e);
            } else {
                log::info!("[AUDIO_THREAD] Set thread priority to Crossplatform(80)");
            }
        } else {
            log::info!("[AUDIO_THREAD] Set thread priority to Max");
        }
    }

    pub fn setup(&self, config: FfiConfig) {
        *self.config.lock() = Some(config.clone());
        self.logger.set_capture_logs(config.capture_logs);

        log::info!("initializing ffi server v{}", env!("CARGO_PKG_VERSION")); // TODO: Move this log
    }

    /// Returns whether the server has been setup.
    pub fn is_setup(&self) -> bool {
        self.config.lock().is_some()
    }

    pub async fn dispose(&'static self) {
        self.logger.set_capture_logs(false);
        log::info!("disposing ffi server");

        // Close all rooms
        let mut rooms = Vec::new();
        for handle in self.ffi_handles.iter_mut() {
            if let Some(handle) = handle.value().downcast_ref::<room::FfiRoom>() {
                rooms.push(handle.clone());
            }
        }

        for room in rooms {
            room.close(self).await;
        }

        // Drop all handles
        *self.config.lock() = None; // Invalidate the config
    }

    pub fn send_event(&self, message: proto::ffi_event::Message) -> FfiResult<()> {
        let start_time = std::time::Instant::now();

        let lock_start = std::time::Instant::now();
        let cb = self
            .config
            .lock()
            .as_ref()
            .map_or_else(|| Err(FfiError::NotConfigured), |c| Ok(c.callback_fn.clone()))?;
        let lock_duration_us = lock_start.elapsed().as_micros() as u64;

        let callback_start = std::time::Instant::now();
        cb(proto::FfiEvent { message: Some(message) });
        let callback_duration_us = callback_start.elapsed().as_micros() as u64;

        let total_duration_us = start_time.elapsed().as_micros() as u64;

        // Only log if total duration exceeds 5ms (5000us)
        if total_duration_us > 5_000 {
            log::warn!(
                "[FFI_CALLBACK] send_event took {}us ({}ms) - lock={}us, callback={}us",
                total_duration_us,
                total_duration_us / 1000,
                lock_duration_us,
                callback_duration_us
            );
        }

        Ok(())
    }

    pub fn next_id(&self) -> FfiHandleId {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Resolves the async_id to use for a request.
    /// Uses the client-provided ID if available, otherwise generates a new one.
    pub fn resolve_async_id(&self, request_async_id: Option<u64>) -> FfiHandleId {
        request_async_id.unwrap_or_else(|| self.next_id())
    }

    pub fn store_handle<T>(&self, id: FfiHandleId, handle: T)
    where
        T: FfiHandle,
    {
        self.ffi_handles.insert(id, Box::new(handle));
    }

    pub fn retrieve_handle<T>(
        &self,
        id: FfiHandleId,
    ) -> FfiResult<MappedRef<'_, u64, Box<dyn FfiHandle>, T>>
    where
        T: FfiHandle,
    {
        if id == INVALID_HANDLE {
            return Err(FfiError::InvalidRequest("handle is invalid".into()));
        }

        let handle =
            self.ffi_handles.get(&id).ok_or(FfiError::InvalidRequest("handle not found".into()))?;

        if !handle.is::<T>() {
            let tyname = std::any::type_name::<T>();
            let msg = format!("handle is not a {}", tyname);
            return Err(FfiError::InvalidRequest(msg.into()));
        }

        let handle = handle.map(|v| v.downcast_ref::<T>().unwrap());
        Ok(handle)
    }

    pub fn take_handle<T>(&self, id: FfiHandleId) -> FfiResult<T>
    where
        T: FfiHandle,
    {
        if id == INVALID_HANDLE {
            return Err(FfiError::InvalidRequest("handle is invalid".into()));
        }

        let (_, handle) = self
            .ffi_handles
            .remove(&id)
            .ok_or(FfiError::InvalidRequest("handle not found".into()))?;

        let handle = handle.downcast::<T>().map_err(|_| {
            let tyname = std::any::type_name::<T>();
            let msg = format!("handle is not a {}", tyname);
            FfiError::InvalidRequest(msg.into())
        })?;
        Ok(*handle)
    }

    pub fn drop_handle(&self, id: FfiHandleId) -> bool {
        let existed = self.ffi_handles.remove(&id).is_some();
        self.handle_dropped_txs.remove(&id);
        return existed;
    }

    pub fn watch_handle_dropped(&self, handle: FfiHandleId) -> oneshot::Receiver<()> {
        // Create vec if not exists
        if self.handle_dropped_txs.get(&handle).is_none() {
            self.handle_dropped_txs.insert(handle, Vec::new());
        }
        let (tx, rx) = oneshot::channel::<()>();
        let mut tx_vec = self.handle_dropped_txs.get_mut(&handle).unwrap();
        tx_vec.push(tx);
        return rx;
    }

    pub fn send_panic(&self, err: Box<dyn Error>) {
        let _ = self.send_event(proto::Panic { message: err.as_ref().to_string() }.into());
    }

    pub fn watch_panic<O>(&'static self, handle: JoinHandle<O>) -> JoinHandle<O>
    where
        O: Send + 'static,
    {
        let handle = self.async_runtime.spawn(async move {
            match handle.await {
                Ok(r) => r,
                Err(e) => {
                    // Forward the panic to the client
                    // Recommended behaviour is to exit the process
                    log::error!("task panicked: {:?}", e);
                    self.send_panic(Box::new(e));
                    panic!("watch_panic: task panicked");
                }
            }
        });
        handle
    }
}
