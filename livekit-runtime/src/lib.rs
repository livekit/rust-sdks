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

#[track_caller]
pub fn spawn<F>(fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    #[cfg(feature = "runtime-tokio")]
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return JoinHandle::Tokio(handle.spawn(fut));
    }

    missing_rt(fut)
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
    type Output = T;

    #[track_caller]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &mut *self {
            #[cfg(feature = "runtime-tokio")]
            Self::Tokio(handle) => {
                Pin::new(handle).poll(cx).map(|res| res.expect("spawned task panicked"))
            }
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
