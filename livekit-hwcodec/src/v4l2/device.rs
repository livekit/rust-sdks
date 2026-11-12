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

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    time::Instant,
};

use thiserror::Error;

use super::sys::{Device, FourCc, Queue};

#[derive(Clone, Debug)]
pub(crate) struct EncoderDevice {
    pub path: PathBuf,
    pub driver: String,
    pub card: String,
    /// `(major << 16) | (minor << 8) | patch`
    pub kernel: u32,
}

impl fmt::Display for EncoderDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (driver {}, card {}, kernel {}.{}.{})",
            self.path.display(),
            self.driver,
            self.card,
            self.kernel >> 16,
            (self.kernel >> 8) & 0xff,
            self.kernel & 0xff
        )
    }
}

#[derive(Debug, Error)]
pub(crate) enum ProbeError {
    #[error("cannot open: {0}")]
    Open(#[source] io::Error),
    #[error("cannot query: {0}")]
    Query(#[source] io::Error),
    #[error("not a multi-planar memory-to-memory device")]
    NotM2m,
    #[error("does not encode {0}")]
    NoCodedFormat(FourCc),
    #[error("does not accept NV12 frames")]
    NoNv12,
}

/// Checks whether `path` is a stateful encoder from NV12 to `coded`.
pub(crate) fn probe(path: &Path, coded: FourCc) -> Result<EncoderDevice, ProbeError> {
    let device = Device::open(path).map_err(ProbeError::Open)?;
    let capability = device.capability().map_err(ProbeError::Query)?;
    if !capability.m2m_mplane {
        return Err(ProbeError::NotM2m);
    }
    // A decoder lists coded formats on its output queue instead.
    if !device.formats(Queue::Capture).map_err(ProbeError::Query)?.contains(&coded) {
        return Err(ProbeError::NoCodedFormat(coded));
    }
    if !device.formats(Queue::Output).map_err(ProbeError::Query)?.contains(&FourCc::NV12) {
        return Err(ProbeError::NoNv12);
    }
    Ok(EncoderDevice {
        path: path.to_owned(),
        driver: capability.driver,
        card: capability.card,
        kernel: capability.version,
    })
}

/// Probes every `/dev/video*` node, logging each outcome.
pub(crate) fn discover(coded: FourCc) -> Vec<EncoderDevice> {
    let started = Instant::now();
    let nodes = video_nodes();
    let mut encoders = Vec::new();
    for path in &nodes {
        match probe(path, coded) {
            Ok(encoder) => {
                log::info!("V4L2 {encoder}: {coded} encoder");
                encoders.push(encoder);
            }
            Err(ProbeError::Open(error)) if error.kind() == io::ErrorKind::PermissionDenied => {
                log::warn!(
                    "V4L2 {}: permission denied, so hardware encoding cannot use it; \
                     add the user to the `video` group",
                    path.display()
                );
            }
            Err(error) => log::info!("V4L2 {}: skipped: {error}", path.display()),
        }
    }
    log::info!(
        "V4L2 probe: {} encoder(s) among {} node(s) in {:?}",
        encoders.len(),
        nodes.len(),
        started.elapsed()
    );
    encoders
}

fn video_nodes() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir("/dev") else {
        return Vec::new();
    };
    let mut nodes: Vec<(u32, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let number = entry.file_name().to_str()?.strip_prefix("video")?.parse().ok()?;
            Some((number, entry.path()))
        })
        .collect();
    nodes.sort();
    nodes.into_iter().map(|(_, path)| path).collect()
}

#[cfg_attr(not(feature = "__lk-vicodec-test"), allow(dead_code, unused_imports))]
#[cfg(test)]
mod tests {
    use super::{
        super::sys::testing::{exclusive, resources, vicodec, FWHT},
        *,
    };

    #[test]
    #[cfg(feature = "__lk-vicodec-test")]
    fn discovery_finds_the_encoder_and_leaves_nothing_open() {
        let _exclusive = exclusive();
        let path = vicodec();
        let baseline = resources(&path);

        let encoder = probe(&path, FWHT).unwrap();
        assert_eq!(encoder.driver, "vicodec");

        let found = discover(FWHT);
        assert_eq!(found.iter().filter(|encoder| encoder.path == path).count(), 1);
        assert!(found.iter().all(|encoder| encoder.driver != "vicodec" || encoder.path == path));
        assert!(matches!(probe(&path, FourCc::H264), Err(ProbeError::NoCodedFormat(_))));

        assert_eq!(resources(&path), baseline);
    }
}
