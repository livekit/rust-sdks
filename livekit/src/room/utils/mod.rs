use std::collections::HashMap;

pub mod handler;

pub fn calculate_changed_attributes(
    old_attributes: HashMap<String, String>,
    new_attributes: HashMap<String, String>,
) -> HashMap<String, String> {
    let old_keys = old_attributes.keys();
    let new_keys = new_attributes.keys();
    let all_keys: Vec<_> = old_keys.chain(new_keys).collect();

    let mut changed: HashMap<String, String> = HashMap::new();
    for key in all_keys {
        let old_value = old_attributes.get(key);
        let new_value = new_attributes.get(key);

        if old_value != new_value {
            match new_value {
                Some(new_value) => {
                    changed.insert(key.clone(), new_value.clone());
                }
                None => {
                    changed.insert(key.clone(), String::new());
                }
            }
        }
    }
    changed
}
pub struct Utf8AwareChunks<'a> {
    bytes: &'a [u8],
    chunk_size: usize,
}

impl<'a> Utf8AwareChunks<'a> {
    fn new(bytes: &'a [u8], chunk_size: usize) -> Self {
        if chunk_size < 4 {
            panic!("chunk_size must be at least 4 due to utf8 encoding rules");
        }
        Utf8AwareChunks { bytes, chunk_size }
    }
}

impl<'a> Iterator for Utf8AwareChunks<'a> {
    type Item = &'a [u8];

    /// Uses the same algorithm as in the LiveKit JS SDK.
    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            return None;
        }

        if self.bytes.len() <= self.chunk_size {
            let chunk = self.bytes;
            self.bytes = &[];
            return Some(chunk);
        }

        let mut k = self.chunk_size;
        while k > 0 {
            let byte = self.bytes[k];
            if (byte & 0xc0) != 0x80 {
                break;
            }
            k -= 1;
        }

        let chunk = &self.bytes[..k];
        self.bytes = &self.bytes[k..];
        Some(chunk)
    }
}

pub trait Utf8AwareChunkExt {
    /// Splits the bytes into chunks of the specified size, ensuring that
    /// UTF-8 character boundaries are respected.
    fn utf8_aware_chunks(&self, chunk_size: usize) -> Utf8AwareChunks;
}

impl Utf8AwareChunkExt for [u8] {
    fn utf8_aware_chunks(&self, chunk_size: usize) -> Utf8AwareChunks {
        Utf8AwareChunks::new(self, chunk_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_chunking() {
        let test_string = "Hello, World!".as_bytes();
        let chunks: Vec<&[u8]> = test_string.utf8_aware_chunks(4).collect();
        assert_eq!(
            chunks,
            [&[72, 101, 108, 108], &[111, 44, 32, 87], &[111, 114, 108, 100], &[33][..]]
        );
    }

    #[test]
    fn test_empty_string_chunking() {
        let empty_string = "".as_bytes();
        let chunks: Vec<&[u8]> = empty_string.utf8_aware_chunks(5).collect();
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_single_character_string_chunking() {
        let single_char = "X".as_bytes();
        let chunks: Vec<&[u8]> = single_char.utf8_aware_chunks(5).collect();
        assert_eq!(chunks, [&[88]]);
    }

    #[test]
    fn test_mixed_string_chunking() {
        let mixed_string = "Hello 👋".as_bytes();
        let chunks: Vec<&[u8]> = mixed_string.utf8_aware_chunks(4).collect();
        assert_eq!(
            chunks,
            [&[0x48, 0x65, 0x6C, 0x6C], &[0x6F, 0x20][..], &[0xF0, 0x9F, 0x91, 0x8B]]
        );
    }
}
