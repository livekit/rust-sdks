// Copyright 2026 LiveKit, Inc.
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

//! The time surface, derived entirely from [`crate::Runtime::sleep`].
//!
//! Nothing here is feature-gated: one implementation serves every backend, and a
//! new backend gets all of it for free. This module compiles with no backend at
//! all, which is what makes a `set_runtime`-only build possible.

use std::{
    fmt,
    future::{poll_fn, Future},
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use crate::BoxFuture;

/// Re-exported so callers have one spelling of "now" that matches [`Sleep::reset`].
///
/// Note this is `std::time::Instant` on every backend. The `tokio` flavor used to
/// re-export `tokio::time::Instant` here.
pub use std::time::Instant;

/// The `Stream` trait, re-exported so implementors don't have to guess which
/// futures crate the active backend pulled in. `tokio_stream::Stream` and
/// `futures::Stream` are both re-exports of this same trait.
pub use futures_core::Stream;

/// A future that completes at a deadline. Created by [`sleep`].
pub struct Sleep {
    fut: BoxFuture,
}

/// A future that completes after `dur`.
pub fn sleep(dur: Duration) -> Sleep {
    Sleep { fut: crate::runtime().sleep(dur) }
}

impl Sleep {
    /// Re-arm this timer to fire at `deadline`, discarding the pending one. A
    /// deadline in the past fires immediately.
    pub fn reset(&mut self, deadline: Instant) {
        self.fut = crate::runtime().sleep(deadline.saturating_duration_since(Instant::now()));
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.fut.as_mut().poll(cx)
    }
}

impl fmt::Debug for Sleep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sleep").finish_non_exhaustive()
    }
}

/// Error returned by [`timeout`] when the deadline elapsed first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TimeoutError;

impl fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("future timed out")
    }
}

impl std::error::Error for TimeoutError {}

/// Run `fut`, giving up after `dur`.
///
/// Deliberately no `Send` bound on `F`: call sites hold non-`Send` futures across
/// this, and neither the old tokio nor the old dispatcher version required one.
pub fn timeout<F: Future>(
    dur: Duration,
    fut: F,
) -> impl Future<Output = Result<F::Output, TimeoutError>> {
    async move {
        let mut fut = std::pin::pin!(fut);
        let mut deadline = sleep(dur);
        poll_fn(move |cx| {
            // Inner future first, so that a future which is ready in the same poll
            // as the timer wins. This is the `select_biased!` ordering the
            // dispatcher backend relied on.
            if let Poll::Ready(value) = fut.as_mut().poll(cx) {
                return Poll::Ready(Ok(value));
            }
            if Pin::new(&mut deadline).poll(cx).is_ready() {
                return Poll::Ready(Err(TimeoutError));
            }
            Poll::Pending
        })
        .await
    }
}

/// What an [`Interval`] does when a tick is delivered late enough that one or more
/// subsequent ticks are already due.
///
/// Semantically equivalent to [tokio's `MissedTickBehavior`].
///
/// [tokio's `MissedTickBehavior`]: https://docs.rs/tokio/1/tokio/time/enum.MissedTickBehavior.html
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum MissedTickBehavior {
    /// Tick as fast as possible until caught up with the original schedule.
    #[default]
    Burst,
    /// Abandon the missed ticks and re-base the schedule on the late tick.
    Delay,
    /// Abandon the missed ticks but stay on the original schedule.
    Skip,
}

/// A periodic ticker.
///
/// Like `tokio::time::interval`, the **first tick completes immediately**; the
/// async-std and dispatcher backends used to wait a full period first. Driven
/// entirely by [`sleep`], so there is no background task to keep alive.
pub struct Interval {
    period: Duration,
    /// Deadline of the tick that [`Interval::tick`] will return next.
    next: Instant,
    /// Timer armed for `next`, kept across calls so a `select!` loop that drops
    /// and re-creates the tick future does not allocate a fresh one every pass.
    timer: Option<Sleep>,
    missed_tick_behavior: MissedTickBehavior,
}

/// Create an [`Interval`] that ticks immediately and then every `period`.
///
/// # Panics
///
/// If `period` is zero, matching `tokio::time::interval`.
pub fn interval(period: Duration) -> Interval {
    assert!(period > Duration::ZERO, "`period` must be non-zero");
    Interval {
        period,
        next: Instant::now(),
        timer: None,
        missed_tick_behavior: MissedTickBehavior::default(),
    }
}

impl Interval {
    /// Wait for the next tick, returning its scheduled deadline.
    ///
    /// Cancel safe: the schedule only advances once the sleep has resolved, so
    /// dropping this future (as `select!` does on every other branch) leaves a
    /// later call waiting for the same deadline.
    pub async fn tick(&mut self) -> Instant {
        let deadline = self.next;
        let now = Instant::now();
        if now < deadline {
            // Reuse the armed timer if this future was dropped and re-created; it
            // resumes toward the same deadline rather than starting over.
            let timer = self.timer.get_or_insert_with(|| sleep(deadline - now));
            timer.await;
        }
        self.timer = None;
        self.next = self.next_deadline(Instant::now(), deadline);
        deadline
    }

    /// Re-base the schedule so the next tick is a full period from now.
    pub fn reset(&mut self) {
        self.next = Instant::now() + self.period;
        // Whatever was armed pointed at the old deadline.
        self.timer = None;
    }

    pub fn set_missed_tick_behavior(&mut self, behavior: MissedTickBehavior) {
        self.missed_tick_behavior = behavior;
    }

    pub fn period(&self) -> Duration {
        self.period
    }

    /// Where the tick that just fired at `fired` puts the following deadline.
    fn next_deadline(&self, now: Instant, fired: Instant) -> Instant {
        let scheduled = fired + self.period;
        if scheduled >= now {
            return scheduled;
        }

        // The tick ran late enough that the following one is already overdue.
        match self.missed_tick_behavior {
            MissedTickBehavior::Burst => scheduled,
            MissedTickBehavior::Delay => now + self.period,
            MissedTickBehavior::Skip => {
                // Round `now` up to the next point on the original grid. The
                // remainder is always < period, so the u64 cast cannot truncate
                // for any period under ~584 years.
                let behind = now.duration_since(fired).as_nanos();
                let remainder = Duration::from_nanos((behind % self.period.as_nanos()) as u64);
                now + (self.period - remainder)
            }
        }
    }
}

impl fmt::Debug for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Interval")
            .field("period", &self.period)
            .field("missed_tick_behavior", &self.missed_tick_behavior)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The deadline arithmetic is pure, so it is tested without a runtime or a
    /// clock. The integration suite covers the part that is genuinely per-backend:
    /// whether a ticker keeps making progress after falling behind.
    fn ticker(behavior: MissedTickBehavior) -> Interval {
        Interval {
            period: Duration::from_secs(1),
            next: Instant::now(),
            timer: None,
            missed_tick_behavior: behavior,
        }
    }

    #[test]
    fn on_schedule_the_behavior_is_irrelevant() {
        for behavior in
            [MissedTickBehavior::Burst, MissedTickBehavior::Delay, MissedTickBehavior::Skip]
        {
            let it = ticker(behavior);
            let fired = Instant::now();
            // Delivered promptly, so the next deadline is simply one period on.
            let now = fired + Duration::from_millis(1);
            assert_eq!(it.next_deadline(now, fired), fired + Duration::from_secs(1));
        }
    }

    #[test]
    fn burst_catches_up_one_tick_at_a_time() {
        let it = ticker(MissedTickBehavior::Burst);
        let fired = Instant::now();
        // 2.5 periods late: three deadlines are already overdue.
        let now = fired + Duration::from_millis(2500);
        // Still the original next deadline, so it fires immediately and keeps
        // firing until caught up.
        assert_eq!(it.next_deadline(now, fired), fired + Duration::from_secs(1));
    }

    #[test]
    fn delay_rebases_on_the_late_tick() {
        let it = ticker(MissedTickBehavior::Delay);
        let fired = Instant::now();
        let now = fired + Duration::from_millis(2500);
        // A full period from now, abandoning the original grid.
        assert_eq!(it.next_deadline(now, fired), now + Duration::from_secs(1));
    }

    #[test]
    fn skip_returns_to_the_original_grid() {
        let it = ticker(MissedTickBehavior::Skip);
        let fired = Instant::now();
        let now = fired + Duration::from_millis(2500);
        // Grid points are fired + n seconds; the next one after `now` is +3s.
        assert_eq!(it.next_deadline(now, fired), fired + Duration::from_secs(3));
    }

    #[test]
    fn skip_lands_on_the_next_grid_point_when_exactly_on_one() {
        let it = ticker(MissedTickBehavior::Skip);
        let fired = Instant::now();
        let now = fired + Duration::from_secs(2);
        assert_eq!(it.next_deadline(now, fired), fired + Duration::from_secs(3));
    }
}
