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

// `Bytes` is a remote type, so each UniFFI component that registers it with
// `custom_type!` emits its own converter — and the two component Swift files are
// compiled into one module, so a second `typealias Bytes` fails to build
// ("invalid redeclaration of 'Bytes'"). Reuse livekit-datatrack's registration
// instead of adding a second one; the type is then emitted once, in the file that
// owns it. Upstream: https://github.com/mozilla/uniffi-rs/issues/2933
uniffi::use_remote_type!(livekit_datatrack::Bytes);
