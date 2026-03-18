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

/// Drains an output event stream until an event matching `$variant` arrives.
/// Other events are silently skipped. Panics if the variant doesn't arrive
/// within the timeout (default 500ms).
macro_rules! expect_event {
    ($output:expr, $variant:path) => {
        expect_event!($output, $variant, std::time::Duration::from_millis(500))
    };
    ($output:expr, $variant:path, $timeout:expr) => {
        tokio::time::timeout($timeout, async {
            loop {
                match futures_util::StreamExt::next(&mut $output)
                    .await
                    .expect("Stream ended before receiving expected event")
                {
                    $variant(e) => break e,
                    _ => {}
                }
            }
        })
        .await
        .expect(concat!("Timed out waiting for ", stringify!($variant)))
    };
}

pub(crate) use expect_event;
