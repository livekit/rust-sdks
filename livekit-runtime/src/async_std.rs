use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

pub type JoinHandle<T> = AsyncJoinHandle<T>;
pub use async_std::future::timeout;
pub use async_std::task::sleep;
use futures::StreamExt;

pub fn spawn<F, T>(future: F) -> AsyncJoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    AsyncJoinHandle { handle: async_std::task::spawn(future) }
}

#[derive(Debug)]
pub struct AsyncJoinHandle<T> {
    handle: async_std::task::JoinHandle<T>,
}

// TODO, determine if this is ok?
#[derive(Debug)]
pub struct JoinError {}

// impl<T> Future for AsyncJoinHandle<T> {
//     type Output = Result<T, JoinError>;

//     fn poll(
//         mut self: std::pin::Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//     ) -> std::task::Poll<Self::Output> {
//         let this = &mut *self;
//         let mut handle = &mut this.handle;

//         let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
//             Pin::new(&mut handle).poll(cx)
//         }));

//         // Result<Poll<T>> -> Poll<Result<T>>

//         match result {
//             Ok(result) => match result {
//                 std::task::Poll::Ready(t) => std::task::Poll::Ready(Ok(t)),
//                 std::task::Poll::Pending => std::task::Poll::Pending,
//             },
//             Err(_) => std::task::Poll::Ready(Err(JoinError {})),
//         }
//     }
// }

impl<T> Future for AsyncJoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = &mut *self;
        let mut handle = &mut this.handle;
        match Pin::new(&mut handle).poll(cx) {
            std::task::Poll::Ready(value) => std::task::Poll::Ready(Ok(value)),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

pub struct Interval {
    duration: Duration,
    timer: async_io::Timer,
}

impl Interval {
    pub fn reset(&mut self) {
        self.timer = async_io::Timer::interval(self.duration);
    }

    pub async fn tick(&mut self) {
        self.timer.next().await;
    }
}

pub fn interval(duration: Duration) -> Interval {
    Interval { duration, timer: async_io::Timer::interval(duration) }
}
