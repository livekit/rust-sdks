use std::time::Duration;

pub type JoinHandle<T> = async_std::task::JoinHandle<T>;
pub use std::time::Instant;
pub use async_std::future::timeout;
pub use async_std::task::spawn;
use futures::{Future, FutureExt, StreamExt};

pub struct Interval {
    duration: Duration,
    timer: async_io::Timer,
}

impl Interval {
    pub fn reset(&mut self) {
        self.timer.set_after(self.duration)
    }

    pub async fn tick(&mut self) -> Instant {
        self.timer.next().await.unwrap()
    }
}

pub fn interval(duration: Duration) -> Interval {
    Interval { duration, timer: async_io::Timer::interval(duration) }
}

pub struct Sleep {
    timer: async_io::Timer
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

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        self.timer.poll_unpin(cx).map(|_| ())
    }
}