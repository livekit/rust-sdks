use async_task::Runnable;
use futures::{select_biased, Future, FutureExt};
use std::{sync::OnceLock, task::Poll, time::Duration};

pub use async_std::net::TcpStream;
pub use std::time::Instant;

/// This is semantically equivalent to Tokio's MissedTickBehavior:
/// https://docs.rs/tokio/1.36.0/tokio/time/enum.MissedTickBehavior.html
pub enum MissedTickBehavior {
    Burst,
    Delay,
    Skip,
}

static DISPATCHER: OnceLock<&'static dyn Dispatcher> = OnceLock::new();

pub trait Dispatcher: 'static + Send + Sync {
    fn dispatch(&self, runnable: Runnable);
    fn dispatch_after(&self, duration: Duration, runnable: Runnable);
}

pub fn set_dispatcher(dispatcher: impl Dispatcher) {
    let dispatcher = Box::leak(Box::new(dispatcher));
    DISPATCHER.set(dispatcher).ok();
}

fn get_dispatcher() -> &'static dyn Dispatcher {
    *DISPATCHER.get().expect("The livekit dispatcher requires a call to set_dispatcher()")
}

#[derive(Debug)]
pub struct JoinHandle<T> {
    task: Option<async_task::Task<T>>,
}

pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + 'static + Send,
    F::Output: 'static + Send,
{
    let dispatcher = get_dispatcher();
    let (runnable, task) = async_task::spawn(future, |runnable| dispatcher.dispatch(runnable));
    runnable.schedule();
    JoinHandle { task: Some(task) }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        self.task.as_mut().expect("poll() should not be called after drop()").poll_unpin(cx)
    }
}

impl<T> Drop for JoinHandle<T> {
    fn drop(&mut self) {
        self.task.take().expect("This is the only place the option is mutated").detach();
    }
}

pub struct Sleep {
    task: async_task::Task<()>,
}

pub fn sleep(time: Duration) -> Sleep {
    let dispatcher = get_dispatcher();
    let (runnable, task) =
        async_task::spawn(async {}, move |runnable| dispatcher.dispatch_after(time, runnable));
    runnable.schedule();

    Sleep { task }
}

impl Sleep {
    pub fn reset(&mut self, _deadline: Instant) {
        // TODO: Check math
        self.task = sleep(Duration::ZERO).task
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        self.task.poll_unpin(cx)
    }
}

pub struct TimeoutError {}

pub fn timeout<T>(
    duration: Duration,
    future: T,
) -> impl Future<Output = Result<T::Output, TimeoutError>>
where
    T: Future,
{
    async move {
        select_biased! {
            res = future.fuse() => Ok(res),
            _ = sleep(duration).fuse() => Err(TimeoutError {}),
        }
    }
}

pub struct Interval {
    duration: Duration,
    timer: Option<Sleep>,
}

pub fn interval(duration: Duration) -> Interval {
    Interval { duration, timer: Some(sleep(duration)) }
}

impl Interval {
    pub fn reset(&mut self) {
        self.timer = Some(sleep(self.duration))
    }

    pub async fn tick(&mut self) -> Instant {
        let timer = self.timer.take().expect("timer should always be set");
        timer.await;
        self.timer = Some(sleep(self.duration));
        Instant::now()
    }

    pub fn set_missed_tick_behavior(&mut self, _: MissedTickBehavior) {
        // noop, this runtime does not support this feature
    }
}

impl Future for Interval {
    type Output = Instant;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        match self.timer.as_mut().expect("timer should always be set").poll_unpin(cx) {
            Poll::Ready(_) => {
                self.timer = Some(sleep(self.duration));
                Poll::Ready(Instant::now())
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
