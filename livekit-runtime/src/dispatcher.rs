pub use async_net::TcpStream;
use futures::{Future, FutureExt};
use std::{marker::PhantomData, time::Duration};
pub use std::time::Instant;

// We need to create:
// - async_io::Timer::interval

#[derive(Debug)]
pub struct JoinHandle<T> {
    handle: PhantomData<T>
}

pub fn spawn<F>(
    future: F,
) -> JoinHandle<F::Output>
where
    F: Future,
{
    todo!()
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        todo!()
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
    async { todo!("async_std::future::timeout") }
}

pub fn interval(duration: Duration) -> Interval {
    todo!()
}

pub struct Interval {
    duration: Duration,
    timer: PhantomData<()>, // TODO!()
}

impl Interval {
    pub fn reset(&mut self) {
        // self.timer.set_after(self.duration)
    }

    pub async fn tick(&mut self) -> Instant {
        todo!()
        // self.timer.next().await.unwrap()
    }
}


pub struct Sleep {
    timer: async_io::Timer,
}

impl Sleep {
    pub fn reset(&mut self, deadline: Instant) {
        self.timer.set_at(deadline)
    }
}

pub fn sleep(duration: Duration) -> Sleep {
    Sleep { timer: async_io::Timer::after(duration) }
}

impl Future for Sleep {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.timer.poll_unpin(cx).map(|_| ())
    }
}
