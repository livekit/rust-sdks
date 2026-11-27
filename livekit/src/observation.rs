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

//! Per-object observation. Values are copied under the object's authoritative
//! locks; child handles and native WebRTC queries are not deep snapshots.
//! Each core write is a commit. Multi-step operations may publish multiple
//! revisions, including equal value content; revisions are not equality keys.
use parking_lot::{Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};
use tokio::sync::Notify;

/// An owned observation of one object's core-owned state.
#[derive(Clone, Debug)]
pub struct StateSnapshot<T> {
    /// Opaque revision belonging only to this object instance.
    pub revision: u64,
    /// Removal is final; reconnect and unpublishing a reusable local track are not.
    pub terminal: bool,
    /// Owned values; any child handles within this value remain live.
    pub value: T,
}

#[derive(Default, Clone, Copy)]
struct Stamp {
    revision: u64,
    terminal: bool,
}

/// Shared only by the authoritative locks of a single object.
#[derive(Default)]
pub(crate) struct StateRevision {
    stamp: Mutex<Stamp>,
    changed: Notify,
}

impl StateRevision {
    /// Caller holds every authoritative read lock represented by `value`, until
    /// this returns. Writers advance the revision before releasing their lock.
    pub(crate) fn snapshot<T>(&self, value: T) -> StateSnapshot<T> {
        let stamp = *self.stamp.lock();
        StateSnapshot { revision: stamp.revision, terminal: stamp.terminal, value }
    }

    fn advance(&self) {
        let mut stamp = self.stamp.lock();
        stamp.revision = stamp.revision.checked_add(1).expect("state revision exhausted");
    }

    /// Call after mutations stop, or while holding authoritative locks whose writers
    /// reject terminal state. Notify only after releasing those locks.
    pub(crate) fn mark_finished(&self) -> bool {
        let mut stamp = self.stamp.lock();
        if stamp.terminal {
            return false;
        }
        stamp.revision = stamp.revision.checked_add(1).expect("state revision exhausted");
        stamp.terminal = true;
        true
    }

    pub(crate) fn notify(&self) {
        self.changed.notify_waiters();
    }
    pub(crate) fn is_terminal(&self) -> bool {
        self.stamp.lock().terminal
    }
    pub(crate) fn finish(&self) {
        if self.mark_finished() {
            self.notify();
        }
    }

    pub(crate) async fn wait(&self, since: Option<u64>) -> bool {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let stamp = self.stamp.lock();
                if since != Some(stamp.revision) {
                    return true;
                }
                if stamp.terminal {
                    return false;
                }
            }
            notified.await;
        }
    }
}

/// A core lock whose writes cannot omit revision publication. There is no
/// mirrored domain state, observer registry, background task or subscriber queue.
pub(crate) struct Observed<T> {
    value: RwLock<T>,
    revision: Arc<StateRevision>,
}
impl<T> Observed<T> {
    pub(crate) fn new(value: T, revision: &Arc<StateRevision>) -> Self {
        Self { value: RwLock::new(value), revision: revision.clone() }
    }
    pub(crate) fn read(&self) -> RwLockReadGuard<'_, T> {
        self.value.read()
    }
    pub(crate) fn write(&self) -> Write<'_, T> {
        Write { guard: Some(self.value.write()), revision: &self.revision, dirty: false }
    }
}
pub(crate) struct Write<'a, T> {
    dirty: bool,
    guard: Option<RwLockWriteGuard<'a, T>>,
    revision: &'a StateRevision,
}
impl<T> Deref for Write<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.guard.as_deref().expect("live write guard")
    }
}
impl<T> DerefMut for Write<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.dirty = true;
        self.guard.as_deref_mut().expect("live write guard")
    }
}
impl<T> Drop for Write<'_, T> {
    fn drop(&mut self) {
        if self.dirty {
            self.revision.advance();
        }
        drop(self.guard.take());
        // Wake outside both locks: a foreign executor may poll inline.
        if self.dirty {
            self.revision.changed.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::Future,
        pin::pin,
        sync::atomic::{AtomicUsize, Ordering},
        task::{Context, Wake, Waker},
    };

    #[derive(Default)]
    struct Wakes(AtomicUsize);
    impl Wake for Wakes {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn independent_observers_register_before_change_and_coalesce() {
        let a = Arc::new(StateRevision::default());
        let b = Arc::new(StateRevision::default());
        let value = Observed::new(vec![0], &a);
        let historical = {
            let value = value.read();
            a.snapshot(value.clone())
        };
        let mut first = pin!(a.wait(Some(0)));
        let mut second = pin!(a.wait(Some(0)));
        let mut other = pin!(b.wait(Some(0)));
        let wakes: [Arc<Wakes>; 3] = std::array::from_fn(|_| Arc::default());
        for (mut wait, wakes) in
            [first.as_mut(), second.as_mut(), other.as_mut()].into_iter().zip(&wakes)
        {
            assert!(wait
                .as_mut()
                .poll(&mut Context::from_waker(&Waker::from(wakes.clone())))
                .is_pending());
        }
        value.write().push(1);
        value.write().push(2);
        assert!(wakes[0].0.load(Ordering::SeqCst) > 0);
        assert!(wakes[1].0.load(Ordering::SeqCst) > 0);
        assert_eq!(wakes[2].0.load(Ordering::SeqCst), 0);
        assert!(first.await && second.await);
        assert_eq!(historical.value, vec![0]);
        let latest = {
            let value = value.read();
            a.snapshot(value.clone())
        };
        assert_eq!(latest.value, vec![0, 1, 2]);
        assert_eq!(latest.revision, 2);
        b.finish();
        assert!(other.await);
        assert!(!b.wait(Some(1)).await);
        assert!(b.wait(None).await);
    }

    #[test]
    fn multiple_authoritative_locks_and_revision_are_coherent() {
        let revision = Arc::new(StateRevision::default());
        let left = Observed::new(0u64, &revision);
        let right = Observed::new(0u64, &revision);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for _ in 0..2000 {
                    *left.write() += 1;
                }
            });
            scope.spawn(|| {
                for _ in 0..2000 {
                    *right.write() += 1;
                }
            });
            for _ in 0..2000 {
                let left = left.read();
                let right = right.read();
                let snapshot = revision.snapshot((*left, *right));
                assert_eq!(snapshot.revision, snapshot.value.0 + snapshot.value.1);
            }
        });
    }

    #[test]
    fn unwinding_writer_still_publishes_and_read_only_guard_does_not() {
        let revision = Arc::new(StateRevision::default());
        let value = Observed::new(0, &revision);
        drop(value.write());
        assert_eq!(revision.snapshot(()).revision, 0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            *value.write() = 1;
            let mut guard = value.write();
            *guard = 2;
            panic!("mutation interrupted");
        }));
        assert!(result.is_err());
        let value = value.read();
        assert_eq!(revision.snapshot(*value).revision, 2);
    }
}
