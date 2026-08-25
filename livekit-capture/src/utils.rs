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

//! Crate-internal utilities.

/// Runs a blocking task on the tokio blocking pool. Panics resume on the
/// caller, and join failures become source errors.
#[cfg(feature = "tokio")]
#[allow(dead_code)]
pub(crate) async fn run_blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, crate::error::SourceError> + Send + 'static,
) -> Result<T, crate::error::SourceError> {
    match tokio::task::spawn_blocking(task).await {
        Ok(result) => result,
        Err(err) if err.is_panic() => std::panic::resume_unwind(err.into_panic()),
        Err(err) => Err(crate::error::SourceError::new(err)),
    }
}

#[cfg(all(test, feature = "tokio"))]
mod tests {
    use super::*;

    #[tokio::test]
    #[should_panic(expected = "boom")]
    async fn propagates_panic() {
        let _ = run_blocking::<()>(|| panic!("boom")).await;
    }

    #[tokio::test]
    async fn returns_result() {
        assert_eq!(run_blocking(|| Ok(7)).await.unwrap(), 7);
    }
}
