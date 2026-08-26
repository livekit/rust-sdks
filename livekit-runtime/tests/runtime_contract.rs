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

//! The contract every `Runtime` backend has to satisfy.
//!
//! Run this file once per backend:
//!
//! ```sh
//! cargo test -p livekit-runtime --no-default-features --features tokio
//! cargo test -p livekit-runtime --no-default-features --features smol
//! cargo test -p livekit-runtime --no-default-features --features async
//! ```
//!
//! Two things are deliberately *not* covered here, because they cannot be
//! reproduced at this layer and still need end-to-end coverage:
//!
//! - Spawning from a thread the runtime has never seen (libwebrtc invokes Rust
//!   callbacks from its own C++ threads). `tokio::task::spawn` panics off-reactor;
//!   async-std and a dispatcher do not.
//! - Task panics. Every backend handles a panicking task differently before it
//!   ever reaches `JoinHandle`'s `expect`.

mod support;

use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use livekit_runtime::{interval, sleep, spawn, timeout, MissedTickBehavior};
use support::block_on;

/// Long enough to survive a loaded CI box, short enough to keep the suite quick.
const TICK: Duration = Duration::from_millis(50);

// ── spawn ─────────────────────────────────────────────────────────────────────

#[test]
fn spawn_delivers_its_output() {
    block_on(async {
        assert_eq!(spawn(async { 6 * 7 }).await, 42);
    });
}

#[test]
fn spawns_make_progress_concurrently() {
    block_on(async {
        let entered = Arc::new(AtomicUsize::new(0));

        // Each task parks until the other has also started. Serialized execution
        // never gets past the first one, and the outer timeout turns that into a
        // failure instead of a hang.
        let task = |entered: Arc<AtomicUsize>| async move {
            entered.fetch_add(1, Ordering::SeqCst);
            while entered.load(Ordering::SeqCst) < 2 {
                sleep(Duration::from_millis(1)).await;
            }
        };

        let a = spawn(task(entered.clone()));
        let b = spawn(task(entered.clone()));

        timeout(TICK * 20, async {
            a.await;
            b.await;
        })
        .await
        .expect("spawned tasks did not run concurrently");
    });
}

#[test]
fn dropping_the_handle_does_not_cancel_the_task() {
    block_on(async {
        let ran = Arc::new(AtomicBool::new(false));

        {
            let ran = ran.clone();
            // Dropped immediately -- the task must still run to completion.
            drop(spawn(async move {
                sleep(TICK).await;
                ran.store(true, Ordering::SeqCst);
            }));
        }

        let deadline = Instant::now() + TICK * 20;
        while !ran.load(Ordering::SeqCst) && Instant::now() < deadline {
            sleep(Duration::from_millis(5)).await;
        }
        assert!(ran.load(Ordering::SeqCst), "detached task never ran to completion");
    });
}

#[test]
fn spawn_works_from_inside_a_spawned_task() {
    block_on(async {
        assert_eq!(spawn(async { spawn(async { 7 }).await + 1 }).await, 8);
    });
}

// ── sleep ─────────────────────────────────────────────────────────────────────

#[test]
fn sleep_waits_at_least_the_requested_duration() {
    block_on(async {
        let started = Instant::now();
        sleep(TICK).await;
        assert!(started.elapsed() >= TICK, "slept only {:?}", started.elapsed());
    });
}

#[test]
fn sleep_zero_completes() {
    block_on(async {
        sleep(Duration::ZERO).await;
    });
}

#[test]
fn sleep_reset_moves_the_deadline() {
    block_on(async {
        let started = Instant::now();
        let mut timer = sleep(TICK);
        timer.reset(Instant::now() + TICK * 3);
        timer.await;
        assert!(started.elapsed() >= TICK * 3, "reset was ignored: {:?}", started.elapsed());
    });
}

// ── timeout ───────────────────────────────────────────────────────────────────

#[test]
fn timeout_passes_through_a_future_that_finishes() {
    block_on(async {
        assert_eq!(timeout(TICK * 20, async { 5 }).await.expect("should not elapse"), 5);
    });
}

#[test]
fn timeout_elapses_on_a_future_that_never_finishes() {
    block_on(async {
        let started = Instant::now();
        let result = timeout(TICK, std::future::pending::<()>()).await;
        assert!(result.is_err(), "pending future did not time out");
        assert!(started.elapsed() >= TICK, "timed out early: {:?}", started.elapsed());
    });
}

#[test]
fn timeout_does_not_fire_when_the_inner_future_wins() {
    block_on(async {
        let result = timeout(TICK * 8, async {
            sleep(TICK).await;
            "done"
        })
        .await;
        assert_eq!(result.expect("inner future should have won"), "done");
    });
}

// ── interval ──────────────────────────────────────────────────────────────────

#[test]
fn interval_first_tick_is_immediate_then_periodic() {
    block_on(async {
        let started = Instant::now();
        let mut ticker = interval(TICK);

        ticker.tick().await;
        assert!(started.elapsed() < TICK, "first tick waited: {:?}", started.elapsed());

        ticker.tick().await;
        assert!(started.elapsed() >= TICK, "second tick was early: {:?}", started.elapsed());

        ticker.tick().await;
        assert!(started.elapsed() >= TICK * 2, "third tick was early: {:?}", started.elapsed());
    });
}

#[test]
fn interval_reset_rebases_the_schedule() {
    block_on(async {
        let mut ticker = interval(TICK);
        ticker.tick().await; // immediate

        let started = Instant::now();
        ticker.reset();
        ticker.tick().await;
        assert!(started.elapsed() >= TICK, "tick after reset was early: {:?}", started.elapsed());
    });
}

#[test]
fn abandoning_a_tick_does_not_slide_the_deadline() {
    block_on(async {
        // `select!` drops the tick future on every pass where another branch wins,
        // which the signal client's ping loop does once per received message. The
        // deadline has to survive that, or the interval slowly drifts to never
        // firing under load.
        let period = TICK * 6;
        let mut ticker = interval(period);
        ticker.tick().await; // immediate

        let started = Instant::now();
        for _ in 0..4 {
            // Each attempt gives up well before the deadline, dropping the future.
            assert!(
                timeout(TICK, ticker.tick()).await.is_err(),
                "tick fired early; test's own timing assumptions are wrong"
            );
        }
        ticker.tick().await;

        let elapsed = started.elapsed();
        // Deadline preserved => ~6 TICK. Restarted on each drop => ~10 TICK.
        assert!(
            elapsed < TICK * 8,
            "abandoning the tick future pushed the deadline out: {elapsed:?}"
        );
    });
}

#[test]
fn interval_recovers_after_falling_behind() {
    block_on(async {
        // The deadline arithmetic for each `MissedTickBehavior` is covered by the
        // unit tests in `src/time.rs`, which need no clock. What is per-backend, and
        // what this checks, is that a ticker whose deadlines have all gone by keeps
        // delivering instead of stalling.
        for behavior in
            [MissedTickBehavior::Burst, MissedTickBehavior::Delay, MissedTickBehavior::Skip]
        {
            let mut ticker = interval(TICK);
            ticker.set_missed_tick_behavior(behavior);

            ticker.tick().await; // immediate
            sleep(TICK * 3).await; // fall several periods behind

            for i in 0..3 {
                timeout(TICK * 20, ticker.tick()).await.unwrap_or_else(|_| {
                    panic!("{behavior:?}: ticker stalled on tick {i} after falling behind")
                });
            }
        }
    });
}

#[test]
#[should_panic(expected = "must be non-zero")]
fn interval_rejects_a_zero_period() {
    // Matches `tokio::time::interval`, which is what production runs today.
    let _ = interval(Duration::ZERO);
}
