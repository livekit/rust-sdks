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

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Mime(Cow<'static, str>);

impl Mime {
    pub const BINARY: Mime = Mime(Cow::Borrowed("application/octet-stream"));
    pub const PLAIN_TEXT: Mime = Mime(Cow::Borrowed("text/plain"));
    pub const JSON: Mime = Mime(Cow::Borrowed("application/json"));
    pub const YAML: Mime = Mime(Cow::Borrowed("application/yaml"));
    pub const PROTOBUF: Mime = Mime(Cow::Borrowed("application/x-protobuf"));
}

impl fmt::Display for Mime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MimeError {
    #[error("Empty string")]
    Empty,
    #[error("Unknown top level type")]
    UnknownTopLevelType,
    #[error("Missing sub-type")]
    MissingSubtype,
    #[error("Sub-type is empty or contains invalid characters")]
    InvalidSubtype,
}

impl FromStr for Mime {
    type Err = MimeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::validate(s)?;
        let mime = Mime(Cow::Owned(s.to_string()));
        Ok(mime)
    }
}

impl Mime {

    fn validate(s: &str) -> Result<(), MimeError> {
        if s.is_empty() {
            Err(MimeError::Empty)?
        }
        let (top_level_type, sub_type) = s.split_once("/").ok_or(MimeError::MissingSubtype)?;
        if !Self::TOP_LEVEL_TYPES.contains(&top_level_type) {
            Err(MimeError::UnknownTopLevelType)?
        }
        if !Self::is_valid_subtype(sub_type) {
            Err(MimeError::InvalidSubtype)?
        }
        Ok(())
    }

    /// Defined top level media types:
    /// https://www.iana.org/assignments/top-level-media-types/top-level-media-types.xhtml
    const TOP_LEVEL_TYPES: &[&str] = &[
        "application",
        "text",
        "image",
        "audio",
        "video",
        "multipart",
        "message",
        "model",
        "haptics",
        "example",
        "font",
    ];

    fn is_valid_subtype(sub_type: &str) -> bool {
        !sub_type.is_empty()
            && sub_type.chars().all(|c| match c {
                'a'..='z'
                | 'A'..='Z'
                | '0'..='9'
                | '!'
                | '#'
                | '$'
                | '&'
                | '^'
                | '_'
                | '.'
                | '+'
                | '-' => true,
                _ => false,
            })
    }
}