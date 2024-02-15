//! This crate intends to abstract over runtime implementations at compile time.
//! The implementation is based on the rt module in sqlx:
//! https://github.com/launchbadge/sqlx/blob/d43257e18abeb4524e541f5a160f8b1fa11e9234/sqlx-core/src/rt/mod.rs

use std::{
    error::Error,
    fmt::Display,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

// interval, Interval,

#[derive(Debug)]
pub struct TimeoutError;

impl Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "operation timed out")?;
        Ok(())
    }
}
impl Error for TimeoutError {}

#[derive(Debug)]
pub enum JoinError {
    Cancelled {
        #[cfg(feature = "runtime-tokio")]
        inner: tokio::task::JoinError,
        _p: PhantomData<()>,
    },
    Panic {
        #[cfg(feature = "runtime-tokio")]
        inner: tokio::task::JoinError,
        _p: PhantomData<()>,
    },
}

impl Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "task failed: {}",
            match self {
                JoinError::Cancelled { .. } => "cancelled",
                JoinError::Panic { .. } => "panicked",
            }
        )?;
        Ok(())
    }
}
impl Error for JoinError {}

pub enum JoinHandle<T> {
    #[cfg(feature = "runtime-tokio")]
    Tokio(tokio::task::JoinHandle<T>),
    // `PhantomData<T>` requires `T: Unpin`
    _Phantom(PhantomData<fn() -> T>),
}

pub enum Interval {
    #[cfg(feature = "runtime-tokio")]
    Tokio(tokio::time::Interval),
    _Phantom,
}

pub enum Runtime {
    #[cfg(feature = "runtime-tokio")]
    Tokio(tokio::runtime::Runtime),
    _Phantom,
}

#[derive(Clone)]
pub enum Handle {
    #[cfg(feature = "runtime-tokio")]
    Tokio(tokio::runtime::Handle),
    _Phantom,
}

pub enum EnterGuard<'a> {
    #[cfg(feature = "runtime-tokio")]
    Tokio(tokio::runtime::EnterGuard<'a>),
    _Phantom,
}

impl Handle {
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        match self {
            #[cfg(feature = "runtime-tokio")]
            Handle::Tokio(handle) => JoinHandle::Tokio(handle.spawn(future)),
            Handle::_Phantom => unreachable!("runtime should have been checked on spawn"),
        }
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        match self {
            #[cfg(feature = "runtime-tokio")]
            Handle::Tokio(handle) => handle.block_on(future),
            Handle::_Phantom => unreachable!("runtime should have been checked on spawn"),
        }
    }
}

impl Runtime {
    pub fn new() -> Self {
        #[cfg(feature = "runtime-tokio")]
        {
            Runtime::Tokio(
                tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap(),
            )
        }

        // TODO(Zed): fix this cfg flag once there's more of them
        #[cfg(not(feature = "runtime-tokio"))]
        {
            return Runtime::_Phantom;
        }
    }

    pub fn handle(&self) -> Handle {
        match self {
            #[cfg(feature = "runtime-tokio")]
            Runtime::Tokio(handle) => Handle::Tokio(handle.handle().clone()),
            Runtime::_Phantom => unreachable!("runtime should have been checked on spawn"),
        }
    }

    pub fn enter(&self) -> EnterGuard {
        match self {
            #[cfg(feature = "runtime-tokio")]
            Runtime::Tokio(runtime) => EnterGuard::Tokio(runtime.enter()),
            Runtime::_Phantom => unreachable!("runtime should have been checked on spawn"),
        }
    }

    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        match self {
            #[cfg(feature = "runtime-tokio")]
            Runtime::Tokio(rt) => JoinHandle::Tokio(rt.spawn(future)),
            Runtime::_Phantom => unreachable!("runtime should have been checked on spawn"),
        }
    }

    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        match self {
            #[cfg(feature = "runtime-tokio")]
            Runtime::Tokio(runtime) => runtime.block_on(future),
            Runtime::_Phantom => unreachable!("runtime should have been checked on spawn"),
        }
    }
}

#[track_caller]
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    #[cfg(feature = "runtime-tokio")]
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return JoinHandle::Tokio(handle.spawn(future));
    }

    missing_rt(future)
}

pub async fn timeout<F: Future>(duration: Duration, f: F) -> Result<F::Output, TimeoutError> {
    #[cfg(feature = "runtime-tokio")]
    if tokio::runtime::Handle::try_current().is_ok() {
        return tokio::time::timeout(duration, f).await.map_err(|_| TimeoutError);
    }

    missing_rt((duration, f))
}

pub async fn sleep(duration: Duration) {
    #[cfg(feature = "runtime-tokio")]
    if tokio::runtime::Handle::try_current().is_ok() {
        return tokio::time::sleep(duration).await;
    }

    missing_rt(duration)
}

#[track_caller]
pub fn interval(duration: Duration) -> Interval {
    #[cfg(feature = "runtime-tokio")]
    if tokio::runtime::Handle::try_current().is_ok() {
        return Interval::Tokio(tokio::time::interval(duration));
    }

    missing_rt(())
}

#[track_caller]
pub fn missing_rt<T>(_unused: T) -> ! {
    if cfg!(feature = "runtime-tokio") {
        panic!("this functionality requires a Tokio context")
    }

    panic!("The a runtime must be enabled")
}

impl<T: Send + 'static> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;

    #[track_caller]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &mut *self {
            #[cfg(feature = "runtime-tokio")]
            Self::Tokio(handle) => Pin::new(handle).poll(cx).map_err(|e| {
                if e.is_panic() {
                    JoinError::Panic { inner: e, _p: PhantomData }
                } else {
                    JoinError::Cancelled { inner: e, _p: PhantomData }
                }
            }),
            Self::_Phantom(_) => {
                let _ = cx;
                unreachable!("runtime should have been checked on spawn")
            }
        }
    }
}

impl<T> std::fmt::Debug for JoinHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(&format!(
            "JoinHandle ({})",
            match self {
                #[cfg(feature = "runtime-tokio")]
                Self::Tokio(_) => "tokio",
                Self::_Phantom(_) => "phantom",
            }
        ))
        .finish()
    }
}

impl Interval {
    pub async fn tick(&mut self) -> Instant {
        match self {
            #[cfg(feature = "runtime-tokio")]
            Interval::Tokio(interval) => interval.tick().await.into(),
            Interval::_Phantom => {
                unreachable!("should have been checked on creation")
            }
        }
    }

    pub fn reset(&mut self) {
        match self {
            #[cfg(feature = "runtime-tokio")]
            Interval::Tokio(interval) => interval.reset(),
            Interval::_Phantom => {
                unreachable!("should have been checked on creation")
            }
        }
    }
}
