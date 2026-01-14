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

use bytes::Bytes;

/// Extension methods for chunking [`Bytes`] into zero-copy payloads.
pub trait BytesChunkExt {
    /// Split into zero-copy chunks of size <= `max_size`.
    ///
    /// # Panics
    /// If `max_size` is equal to zero.
    ///
    fn into_chunks(self, max_size: usize) -> ChunkIter;
}

impl BytesChunkExt for Bytes {
    fn into_chunks(self, max_size: usize) -> ChunkIter {
        assert_ne!(max_size, 0, "Zero chunk size is invalid");
        ChunkIter { source: self, max_size }
    }
}

/// An iterator over chunks of a certain size.
///
/// Internally, this uses [`Bytes::split_to`], an O(1) operation.
///
pub struct ChunkIter {
    source: Bytes,
    max_size: usize,
}

impl Iterator for ChunkIter {
    type Item = Bytes;

    fn next(&mut self) -> Option<Self::Item> {
        if self.source.is_empty() {
            return None;
        }
        let n = self.max_size.min(self.source.len());
        Some(self.source.split_to(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_matrix;

    #[test]
    fn test_empty_source() {
        let source = Bytes::new();
        let chunks: Vec<_> = source.into_chunks(256).collect();
        assert!(chunks.is_empty())
    }

    #[test_matrix([1, 128, 333], [1, 64, 128, 256, 123])]
    fn test_chunks(
        chunk_size: usize,
        source_size: usize,
    ) {
        let source = Bytes::from(vec![0xCC; source_size]);
        let chunks: Vec<_> = source.into_chunks(chunk_size).collect();

        let expected_chunks = (source_size + chunk_size - 1) / chunk_size;
        assert_eq!(chunks.len(), expected_chunks);

        // All but last chunk's length match chunks size
        assert!(chunks[..chunks.len().saturating_sub(1)].iter().all(|c| c.len() == chunk_size));

        // Last is either full (divisible) or the remainder.
        let expected_last_len = if source_size % chunk_size == 0 {
            chunk_size.min(source_size)
        } else {
            source_size % chunk_size
        };
        assert_eq!(chunks.last().unwrap().len(), expected_last_len);
    }
}
