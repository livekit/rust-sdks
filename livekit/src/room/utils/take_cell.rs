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

use parking_lot::Mutex;
use std::{fmt::Debug, sync::Arc};

/// A thread-safe cell that holds a value which can be taken exactly once.
/// After taking the value, subsequent attempts to take it will return `None`.
pub struct TakeCell<T> {
    inner: Arc<Mutex<Option<T>>>,
}

impl<T> TakeCell<T> {
    pub(crate) fn new(value: T) -> Self {
        Self { inner: Arc::new(Mutex::new(Some(value))) }
    }
    /// Take ownership of the value in the cell. If the value has,
    /// already been taken, the result is `None`.
    pub fn take(&self) -> Option<T> {
        self.inner.lock().take()
    }

    /// Returns whether or not the value has been taken.
    pub fn is_taken(&self) -> bool {
        self.inner.lock().is_none()
    }
}

impl<T> Debug for TakeCell<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.lock();
        f.debug_struct("TakeCell")
            .field(
                "value",
                &inner.as_ref().map(|v| v as &dyn Debug).unwrap_or(&"<taken>" as &dyn Debug),
            )
            .finish()
    }
}

impl<T> Clone for TakeCell<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_take() {
        let cell = TakeCell::new(1);
        assert_eq!(cell.is_taken(), false);
        assert_eq!(cell.take(), Some(1));
        assert_eq!(cell.take(), None);
        assert_eq!(cell.is_taken(), true);
    }
}
