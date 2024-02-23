pub use tokio::task::spawn;
pub use tokio::time::Instant;
pub use tokio::time::sleep;
pub use tokio::time::timeout;

pub type JoinHandle<T> = TokioJoinHandle<T>;
pub type Interval = tokio::time::Interval;

struct TokioJoinHandle<T> {
    handle: JoinHandle<T>,
}

pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    TokioJoinHandle { handle: tokio::task::spawn(future) }
}

impl<T> Future for TokioJoinHandle<T> {
    type Output = T;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = &mut *self;
        let mut handle = &mut this.handle;
        match Pin::new(&mut handle).poll(cx) {
            std::task::Poll::Ready(value) => std::task::Poll::Ready(value.expect("Tasks should not panic")),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

// TODO: Is this ok? Or should we have some kind of seperate compatibility layer?
// TODO: Confirm that this matches the async-io implementation
pub fn interval(duration: Duration) -> Interval {
    let timer = tokio::time::interval(duration);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    timer
}
