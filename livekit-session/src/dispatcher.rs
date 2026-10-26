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

use std::sync::{Mutex, PoisonError};

use tokio::sync::mpsc;

/// Fans values out to any number of subscribers, each on its own unbounded
/// channel. A subscriber whose receiver is dropped is removed on the next
/// dispatch.
///
/// `livekit` has the same type, but keeps it private.
pub(crate) struct Dispatcher<T> {
    senders: Mutex<Vec<mpsc::UnboundedSender<T>>>,
}

impl<T> Default for Dispatcher<T> {
    fn default() -> Self {
        Self { senders: Mutex::default() }
    }
}

impl<T: Clone> Dispatcher<T> {
    pub(crate) fn register(&self) -> mpsc::UnboundedReceiver<T> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.senders.lock().unwrap_or_else(PoisonError::into_inner).push(tx);
        rx
    }

    pub(crate) fn dispatch(&self, value: &T) {
        self.senders
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|sender| sender.send(value.clone()).is_ok());
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.senders.lock().unwrap_or_else(PoisonError::into_inner).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_receiver_is_pruned_on_the_next_dispatch() {
        let dispatcher = Dispatcher::default();
        let mut kept = dispatcher.register();
        let dropped = dispatcher.register();
        assert_eq!(dispatcher.len(), 2);

        drop(dropped);
        assert_eq!(dispatcher.len(), 2, "pruning happens on dispatch, not on drop");
        dispatcher.dispatch(&1);
        assert_eq!(dispatcher.len(), 1);
        assert_eq!(kept.try_recv(), Ok(1));
    }
}
