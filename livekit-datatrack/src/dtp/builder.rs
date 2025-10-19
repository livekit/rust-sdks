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

use super::common::{consts::*, Encryption, Iv};
use std::io::{Cursor, Write};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BuildError {
    #[error("Buffer not long enough to contain packet")]
    TooShort,
}

#[derive(Clone)]
pub struct DtpBuilder<'a> {
    is_final: bool,
    encryption: Option<Encryption<'a>>,
    payload: Option<&'a [u8]>,
    track_handle: u16,
    sequence: u16,
    timestamp: Option<u32>,
    user_timestamp: Option<u64>,
}

impl<'a> DtpBuilder<'a> {
    pub fn new() -> Self {
        DtpBuilder {
            is_final: true,
            encryption: None,
            payload: None,
            track_handle: 0,
            sequence: 0,
            timestamp: None,
            user_timestamp: None,
        }
    }

    pub fn is_final(mut self, is_final: bool) -> Self {
        self.is_final = is_final;
        self
    }

    pub fn track_handle(mut self, track_handle: u16) -> Self {
        self.track_handle = track_handle;
        self
    }

    pub fn sequence(mut self, sequence: u16) -> Self {
        self.sequence = sequence;
        self
    }

    pub fn encryption(mut self, key_index: u8, iv: &'a Iv) -> Self {
        let encryption = Encryption { key_index, iv };
        self.encryption = encryption.into();
        self
    }

    pub fn timestamp(mut self, timestamp: u32) -> Self {
        self.timestamp = timestamp.into();
        self
    }

    pub fn user_timestamp(mut self, user_timestamp: u64) -> Self {
        self.user_timestamp = user_timestamp.into();
        self
    }

    pub fn payload(mut self, payload: &'a [u8]) -> Self {
        self.payload = Some(payload);
        self
    }

    fn extension_len(&self) -> usize {
        let mut len = 0;
        if self.timestamp.is_some() {
            len += TIMESTAMP_EXT_LEN;
        }
        if self.user_timestamp.is_some() {
            len += USER_TIMESTAMP_EXT_LEN;
        }
        if self.encryption.is_some() {
            len += E2EE_EXT_LEN;
        }
        len
    }

    fn built_len(&self) -> usize {
        let mut len = BASE_HEADER_LEN + self.extension_len();
        if let Some(payload) = self.payload {
            len += payload.len();
        }
        len
    }

    fn build_into_unchecked(&self, target: &mut [u8]) -> usize {
        target[0] = SUPPORTED_VERSION << VERSION_SHIFT;
        if self.is_final {
            target[0] |= 1 << FINAL_FLAG_SHIFT;
        }
        if self.encryption.is_some() {
            target[0] |= 1 << E2EE_FLAG_SHIFT;
        }
        if self.timestamp.is_some() {
            target[0] |= 1 << TS_FLAG_SHIFT;
        }
        if self.user_timestamp.is_some() {
            target[0] |= 1 << UTS_FLAG_SHIFT;
        }
        target[1] = 0; // reserved
        target[2] = 0; // reserved
        target[EXT_WORDS_OFFSET] = (self.extension_len() / 4) as u8;
        target[TRACK_HANDLE_OFFSET..(TRACK_HANDLE_OFFSET + TRACK_HANDLE_LEN)]
            .copy_from_slice(&self.track_handle.to_be_bytes());
        target[SEQUENCE_OFFSET..(SEQUENCE_OFFSET + SEQUENCE_LEN)]
            .copy_from_slice(&self.sequence.to_be_bytes());

        let mut cursor = Cursor::new(target);
        cursor.set_position(EXT_START_OFFSET as u64);
        if let Some(timestamp) = self.timestamp {
            cursor.write(&timestamp.to_be_bytes()).unwrap();
        }
        if let Some(user_timestamp) = self.user_timestamp {
            cursor.write(&user_timestamp.to_be_bytes()).unwrap();
        }
        if let Some(encryption) = &self.encryption {
            cursor.write(encryption.iv).unwrap();
            cursor.write(&[0x00; 3]).unwrap(); // reserved
            cursor.write(&[encryption.key_index]).unwrap();
        }
        if let Some(payload) = self.payload {
            cursor.write(payload).unwrap();
        }
        cursor.position() as usize
    }

    pub fn build_into(&self, target: &mut [u8]) -> Result<usize, BuildError> {
        let built_len = self.built_len();
        if target.len() < built_len {
            Err(BuildError::TooShort)?
        }
        let bytes_written = self.build_into_unchecked(target);
        assert_eq!(bytes_written, built_len);
        Ok(bytes_written)
    }

    pub fn build(&self) -> Vec<u8> {
        let built_len = self.built_len();
        let mut target = vec![0; built_len];
        let bytes_written = self.build_into_unchecked(&mut target);
        assert_eq!(bytes_written, built_len);
        target
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    pub fn test_build_into_too_short() {
        let mut buffer = [0; 4];
        let result = DtpBuilder::new().build_into(&mut buffer);
        assert!(matches!(result, Err(BuildError::TooShort)));
    }

    #[test]
    pub fn test_build() {
        let packet = DtpBuilder::new()
            .is_final(true)
            .track_handle(1195)
            .sequence(1234)
            .encryption(128, &[0xFA; 12])
            .timestamp(16_910_400)
            .user_timestamp(72_058_693_566_333_184)
            .payload(&[0xFA, 0xAF])
            .build();

        assert_eq!(packet.len(), 38, "Unexpected length");

        let mut cursor = Cursor::new(packet.as_slice());
        assert_eq!(cursor.read_n(), [0x1E]);
        assert_eq!(cursor.read_n(), [0x00; 2], "Reserved bytes should be zero");
        assert_eq!(cursor.read_n(), [0x07], "Extension word count incorrect");
        assert_eq!(cursor.read_n(), [0x04, 0xAB], "Track handle incorrect");
        assert_eq!(cursor.read_n(), [0x04, 0xD2], "Sequence incorrect");

        assert_eq!(cursor.read_n(), [0x01, 0x02, 0x08, 0x40], "Timestamp incorrect");
        assert_eq!(
            cursor.read_n(),
            [0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00],
            "User timestamp incorrect"
        );
        assert_eq!(cursor.read_n(), [0xFA; 12], "IV incorrect");
        assert_eq!(cursor.read_n(), [0x00; 3], "Expected reserved bytes after IV");
        assert_eq!(cursor.read_n(), [0x80], "Expected reserved bytes after IV");
        assert_eq!(cursor.read_n(), [0xFA, 0xAF], "Payload incorrect");
    }

    trait ReadNExt {
        fn read_n<const N: usize>(&mut self) -> [u8; N];
    }
    impl ReadNExt for Cursor<&[u8]> {
        fn read_n<const N: usize>(&mut self) -> [u8; N] {
            let mut buf = [0u8; N];
            self.read_exact(&mut buf).unwrap();
            buf
        }
    }
}
