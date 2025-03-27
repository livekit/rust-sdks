// Copyright 2025 LiveKit, Inc.
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

use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::Arc,
};

pub type HandlerReturn<R> = Pin<Box<dyn Future<Output = R> + Send>>;
type Handler<A, R> = Arc<dyn Fn(A) -> HandlerReturn<R> + Send + Sync>;

/// A registry for async handlers.
///
/// # Type Parameters
///
/// * `A` - The argument type that will be passed to handlers.
/// * `R` - The return type from handlers.
///
pub struct AsyncHandlerRegistry<A, R>
where
    A: Send + 'static,
    R: Send + 'static,
{
    inner: HashMap<String, Entry<A, R>>,
}

enum Entry<A, R>
where
    A: Send + 'static,
    R: Send + 'static,
{
    Handler(Handler<A, R>),
    Queue(VecDeque<A>),
}

impl<A, R> AsyncHandlerRegistry<A, R>
where
    A: Send + 'static,
    R: Send + 'static,
{
    pub fn preregister(&mut self, key: &str) -> bool {
        if self.inner.contains_key(key) {
            return false;
        }

        self.inner.insert(key.to_string(), Entry::Queue(VecDeque::new()));
        true
    }

    pub fn register<F>(&mut self, key: &str, handler: F) -> bool
    where
        F: Fn(A) -> HandlerReturn<R> + Send + Sync + 'static,
    {
        match self.inner.get_mut(key) {
            None => {
                self.inner.insert(key.to_string(), Entry::Handler(Arc::new(handler)));
                true
            }
            Some(entry) => match entry {
                Entry::Queue(queue) => {
                    while let Some(args) = queue.pop_front() {
                        tokio::spawn(handler(args));
                    }
                    *entry = Entry::Handler(Arc::new(handler));
                    true
                }
                Entry::Handler(_) => false,
            },
        }
    }

    pub fn dispatch(&mut self, key: &str, args: A) -> bool {
        match self.inner.get_mut(key) {
            Some(Entry::Handler(handler)) => {
                tokio::spawn(handler(args));
                true
            }
            Some(Entry::Queue(queue)) => {
                queue.push_back(args);
                true
            }
            None => false,
        }
    }

    pub fn unregister(&mut self, key: &str) -> bool {
        self.inner.remove(key).is_some()
    }
}

impl<A, R> Default for AsyncHandlerRegistry<A, R>
where
    A: Send + 'static,
    R: Send + 'static,
{
    fn default() -> Self {
        Self { inner: HashMap::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::sync::Notify;

    const TOPIC: &str = "some-topic";

    #[tokio::test]
    async fn test_dispatch() {
        let mut registry = AsyncHandlerRegistry::<(), ()>::default();
        assert!(registry.preregister(TOPIC));
        assert!(!registry.preregister(TOPIC), "cannot preregister twice");

        let counter = Arc::new(AtomicU32::new(0));
        let notify = Arc::new(Notify::new());
        let notify_clone = notify.clone();
        let counter_clone = counter.clone();

        let dispatch_count = 3;
        for _ in 0..dispatch_count {
            assert!(registry.dispatch(TOPIC, ()));
        }

        assert!(registry.register(TOPIC, move |()| {
            let notify = notify_clone.clone();
            let counter = counter_clone.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                if counter.load(Ordering::SeqCst) == dispatch_count {
                    notify.notify_one();
                }
            })
        }));
        notify.notified().await;
    }

    #[test]
    fn test_dispatch_fail() {
        let mut registry = AsyncHandlerRegistry::<(), ()>::default();
        assert!(!registry.dispatch(TOPIC, ()), "should not dispatch without [pre]registration");
    }

    #[test]
    fn test_unregister() {
        let mut registry = AsyncHandlerRegistry::<(), ()>::default();
        assert!(registry.register(TOPIC, move |()| {
            Box::pin(async move { panic!("handler should not be called") })
        }));
        assert!(registry.unregister(TOPIC));
        registry.dispatch(TOPIC, ());
    }
}
