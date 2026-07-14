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

//! Shared blocking-I/O helpers for the encoded ingest sources.

use std::io::{self, Read};

/// Reads exactly `buf.len()` bytes, returning `Ok(false)` when the stream
/// ends cleanly before the first byte and `UnexpectedEof` when it ends
/// mid-buffer.
pub(crate) fn read_exact_or_clean_eof(reader: &mut impl Read, buf: &mut [u8]) -> io::Result<bool> {
    let mut offset = 0;
    while offset < buf.len() {
        match reader.read(&mut buf[offset..])? {
            0 if offset == 0 => return Ok(false),
            0 => return Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
            read => offset += read,
        }
    }
    Ok(true)
}
