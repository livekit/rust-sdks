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

//! The opt-in body reader shared by native streaming and buffered host views.

use std::io::Write;

use crate::{Header, HttpResponse, TransportError};

/// Owns received headers and a decoded prefix, preserving retry hints on overflow.
pub(crate) struct BodyReader {
    status: u16,
    headers: Vec<Header>,
    plain: LimitedBody,
    gzip: Option<flate2::write::MultiGzDecoder<LimitedBody>>,
}

impl BodyReader {
    /// Select decoding from headers that describe the actual returned body bytes.
    pub(crate) fn new(
        status: u16,
        headers: Vec<Header>,
        limit: u64,
    ) -> Result<Self, TransportError> {
        // Field lines combine into one comma list (RFC 9110 §5.3); `identity` and empty elements
        // add no coding. ponytail: one gzip layer only; stacked codings are rejected.
        let mut gzip = false;
        for coding in headers
            .iter()
            .filter(|h| h.name.eq_ignore_ascii_case("content-encoding"))
            .flat_map(|h| h.value.split(','))
            .map(str::trim)
            .filter(|c| !c.is_empty() && !c.eq_ignore_ascii_case("identity"))
        {
            if gzip || !coding.eq_ignore_ascii_case("gzip") {
                return Err(TransportError::Other(
                    "unsupported bounded response content encoding".into(),
                ));
            }
            gzip = true;
        }
        Ok(Self {
            status,
            headers,
            plain: LimitedBody { bytes: Vec::new(), limit, overflow: false },
            gzip: gzip.then(|| {
                flate2::write::MultiGzDecoder::new(LimitedBody {
                    bytes: Vec::new(),
                    limit,
                    overflow: false,
                })
            }),
        })
    }

    /// Consume one available chunk, stopping before bytes beyond the decoded limit are stored.
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<(), TransportError> {
        let result = match self.gzip.as_mut() {
            Some(decoder) => decoder.write_all(chunk),
            None => self.plain.write_all(chunk),
        };
        result.map_err(|error| {
            let overflow = self.gzip.as_ref().map_or(self.plain.overflow, |d| d.get_ref().overflow);
            if overflow {
                TransportError::ResponseTooLarge { headers: std::mem::take(&mut self.headers) }
            } else {
                TransportError::Other(error.to_string())
            }
        })
    }

    /// Validate gzip completion and return a complete decoded response, never a truncated one.
    pub(crate) fn finish(mut self) -> Result<HttpResponse, TransportError> {
        let body = match self.gzip {
            Some(mut decoder) => {
                if let Err(error) = decoder.try_finish() {
                    return Err(if decoder.get_ref().overflow {
                        TransportError::ResponseTooLarge { headers: self.headers }
                    } else {
                        TransportError::Other(error.to_string())
                    });
                }
                let decoded = decoder.finish().map_err(|e| TransportError::Other(e.to_string()))?;
                self.headers.retain(|h| {
                    !h.name.eq_ignore_ascii_case("content-encoding")
                        && !h.name.eq_ignore_ascii_case("content-length")
                });
                decoded.bytes
            }
            None => self.plain.bytes,
        };
        Ok(HttpResponse { status: self.status, headers: self.headers, body })
    }
}

/// Owns only the accepted decoded prefix; never appends bytes beyond the requested limit.
struct LimitedBody {
    bytes: Vec<u8>,
    limit: u64,
    overflow: bool,
}

impl std::io::Write for LimitedBody {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.limit.saturating_sub(self.bytes.len() as u64) {
            self.overflow = true;
            return Err(std::io::Error::other("response body exceeds requested limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(lines: &[&str], wire: &[u8]) -> Result<HttpResponse, TransportError> {
        let headers = lines
            .iter()
            .map(|v| Header { name: "Content-Encoding".into(), value: (*v).into() })
            .collect();
        let mut body = BodyReader::new(200, headers, 1024)?;
        body.push(wire)?;
        body.finish()
    }

    #[test]
    fn content_coding_list_allows_identity_and_one_gzip_layer() {
        let mut zip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        zip.write_all(b"ok").expect("gzip");
        let gz = zip.finish().expect("finish");
        for lines in [&["gzip"][..], &["identity, gzip"], &["identity", "gzip"], &["gzip,"]] {
            assert_eq!(read(lines, &gz).expect("decodes").body, b"ok", "{lines:?}");
        }
        for lines in [&["br"][..], &["gzip, gzip"], &["gzip", "gzip"]] {
            assert!(matches!(read(lines, &gz), Err(TransportError::Other(_))), "{lines:?}");
        }
    }
}
