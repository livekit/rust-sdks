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

use bytes::Bytes;

// `Bytes` is registered once in livekit-common so every component that needs it borrows the
// same converter; registering it here as well would emit a second `public typealias Bytes`
// into this component's Swift file, and both files compile into one module.
// Upstream: https://github.com/mozilla/uniffi-rs/issues/2933
uniffi::use_remote_type!(livekit_common::Bytes);
