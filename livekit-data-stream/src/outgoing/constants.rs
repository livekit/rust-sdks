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

/// Max chunk content size AND the header-packet MTU budget. Kept below the ~16 KB
/// data-channel MTU for protocol/E2EE framing headroom.
pub(crate) const STREAM_CHUNK_SIZE_BYTES: usize = 15000;

// Default MIME type to use for byte streams.
pub(crate) static BYTE_MIME_TYPE: &str = "application/octet-stream";

/// Default MIME type to use for text streams.
pub(crate) static TEXT_MIME_TYPE: &str = "text/plain";

/// Default name for `send_bytes` byte-stream headers.
pub(crate) static BYTE_DEFAULT_NAME: &str = "unknown";
