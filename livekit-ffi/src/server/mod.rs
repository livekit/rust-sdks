use crate::{proto, FfiCallbackFn, INVALID_HANDLE};
use crate::{FfiError, FfiHandleId, FfiResult};
use dashmap::DashMap;
use downcast_rs::{impl_downcast, Downcast};
use lazy_static::lazy_static;
use parking_lot::deadlock;
use parking_lot::Mutex;
use prost::Message;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

pub mod audio_source;
pub mod audio_stream;
pub mod participant;
pub mod room;
pub mod track;
pub mod video_source;
pub mod video_stream;

mod requests;
#[cfg(test)]
mod tests;

lazy_static! {
    pub static ref FFI_SERVER: FfiServer = FfiServer::default();
}

pub struct FfiConfig {
    callback_fn: FfiCallbackFn,
}

/// To make sure we use the right types, only types that implement this trait
/// can be stored inside the FfiServer.
pub trait FfiHandle: Downcast + Send + Sync {}

impl_downcast!(FfiHandle);

pub struct FfiServer {
    /// Store all Ffi handles inside an HashMap, if this isn't efficient enough
    /// We can still use Box::into_raw & Box::from_raw in the future (but keep it safe for now)
    ffi_handles: DashMap<FfiHandleId, Box<dyn FfiHandle>>,
    async_runtime: tokio::runtime::Runtime,

    next_id: AtomicU64,
    config: Mutex<Option<FfiConfig>>,
}

impl Default for FfiServer {
    fn default() -> Self {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

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
            async_runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
            config: Default::default(),
        }
    }
}

// Using &'static self inside the implementation, not sure if this is really idiomatic
// It simplifies the code a lot tho. In most cases the server is used until the end of the process
impl FfiServer {
    pub async fn dispose(&'static self) {
        log::info!("disposing the FfiServer, closing all rooms...");

        // Close all rooms
        let mut rooms = Vec::new();
        for handle in self.ffi_handles.iter_mut() {
            if let Some(handle) = handle.value().downcast_ref::<room::HandleType>() {
                rooms.push(handle.clone());
            }
        }

        for room in rooms {
            room.close().await;
        }

        // Drop all handles
        self.ffi_handles.clear();
        *self.config.lock() = None; // Invalidate the config
    }

    pub async fn send_event(&'static self, message: proto::ffi_event::Message) -> FfiResult<()> {
        let callback_fn = self
            .config
            .lock()
            .as_ref()
            .map_or_else(|| Err(FfiError::NotConfigured), |c| Ok(c.callback_fn))?;

        let message = proto::FfiEvent {
            message: Some(message),
        }
        .encode_to_vec();

        let cb_task = self.async_runtime.spawn_blocking(move || unsafe {
            callback_fn(message.as_ptr(), message.len());
        });

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(2)) => {
                log::error!("sending an event to the foreign language took too much time, is your callback function blocking?");
            }
            _ = cb_task => {}
        }

        Ok(())
    }

    pub fn next_id(&'static self) -> FfiHandleId {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn store_handle<T>(&'static self, id: FfiHandleId, handle: T)
    where
        T: FfiHandle,
    {
        self.ffi_handles.insert(id, Box::new(handle));
    }

    pub fn retrieve_handle<T>(&'static self, id: FfiHandleId) -> FfiResult<&T>
    where
        T: FfiHandle,
    {
        if id == INVALID_HANDLE {
            return Err(FfiError::InvalidRequest("handle is invalid"));
        }

        let handle = self
            .ffi_handles
            .get(&id)
            .ok_or(FfiError::InvalidRequest("handle not found"))?;

        let handle = handle
            .downcast_ref::<T>()
            .ok_or(FfiError::InvalidRequest(&format!(
                "handle is not a {}",
                std::any::type_name::<T>()
            )))?;

        Ok(handle)
    }
}
