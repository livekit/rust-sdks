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

//! The SoX resampler, exposed over UniFFI.
//!
//! This is the UniFFI counterpart of the `new_sox_resampler` /
//! `push_sox_resampler` / `flush_sox_resampler` protobuf handlers in
//! [`crate::server::requests`], and the first stateful type to cross the UniFFI
//! boundary from this crate. The state itself is unchanged and unmoved: it stays
//! an `Arc<Mutex<crate::server::resampler::SoxResampler>>` in the
//! [`FFI_SERVER`] handle map, so a resampler created here can be driven through
//! the legacy C ABI by handle id, and vice versa.
//!
//! The buffer contract also matches the C ABI deliberately: `push` takes a raw
//! pointer and `push`/`flush` return one. See [`SoxResamplerOutput`] for the
//! lifetime rules that come with that.

use std::{mem::size_of, slice, sync::Arc};

use parking_lot::Mutex;

use super::backed_by_ffi_handle::BackedByFfiHandle;
use crate::{proto, server::resampler, FfiError, FfiHandleId, FFI_SERVER};

/// Sample layout, mirroring [`proto::SoxResamplerDataType`].
///
/// Declared separately rather than exported from the generated protobuf types so
/// the UniFFI surface owns its own naming and is free to diverge from the wire
/// enum later.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoxResamplerDataType {
    /// Channels interleaved in one buffer (`SOXR_INT16_I`).
    Int16Interleaved,
    /// Channels split across separate buffers (`SOXR_INT16_S`).
    Int16Split,
}

impl From<SoxResamplerDataType> for proto::SoxResamplerDataType {
    fn from(value: SoxResamplerDataType) -> Self {
        match value {
            SoxResamplerDataType::Int16Interleaved => Self::SoxrDatatypeInt16i,
            SoxResamplerDataType::Int16Split => Self::SoxrDatatypeInt16s,
        }
    }
}

/// Quality preset, mirroring [`proto::SoxQualityRecipe`].
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoxQualityRecipe {
    Quick,
    Low,
    Medium,
    High,
    VeryHigh,
}

impl From<SoxQualityRecipe> for proto::SoxQualityRecipe {
    fn from(value: SoxQualityRecipe) -> Self {
        match value {
            SoxQualityRecipe::Quick => Self::SoxrQualityQuick,
            SoxQualityRecipe::Low => Self::SoxrQualityLow,
            SoxQualityRecipe::Medium => Self::SoxrQualityMedium,
            SoxQualityRecipe::High => Self::SoxrQualityHigh,
            SoxQualityRecipe::VeryHigh => Self::SoxrQualityVeryhigh,
        }
    }
}

/// Errors from the UniFFI resampler surface.
///
/// A dedicated type rather than [`FfiError`], which is not a `uniffi::Error`.
/// The payload field is named `reason` and not `message`: in Kotlin a UniFFI
/// error field called `message` collides with `Throwable.message` and cannot be
/// renamed away (see the UniFFI notes in the repo's `AGENTS.md`).
#[derive(uniffi::Error, thiserror::Error, Debug)]
pub enum SoxResamplerError {
    #[error("failed to create resampler: {reason}")]
    Create { reason: String },
    #[error("resampler operation failed: {reason}")]
    Process { reason: String },
    #[error("invalid ffi handle: {reason}")]
    InvalidHandle { reason: String },
}

impl From<FfiError> for SoxResamplerError {
    fn from(err: FfiError) -> Self {
        Self::InvalidHandle { reason: err.to_string() }
    }
}

/// Resampled output, as a borrowed pointer into the resampler's internal buffer.
///
/// # Safety
///
/// `output_ptr` is only valid until the next `push` or `flush` on the same
/// handle — the buffer is reused, and `push` may reallocate it. Copy the samples
/// out before calling again. This matches the legacy C ABI's
/// `PushSoxResamplerResponse`.
#[derive(uniffi::Record, Clone, Copy, Debug)]
pub struct SoxResamplerOutput {
    /// `*const i16`, or 0 when no samples were produced.
    pub output_ptr: u64,
    /// Length of the output in **bytes**, matching the C ABI.
    pub size: u32,
}

impl SoxResamplerOutput {
    fn from_samples(samples: &[i16]) -> Self {
        // An empty slice's `as_ptr` is dangling-but-aligned rather than null, so
        // report the empty case as a null pointer instead of handing a foreign
        // caller something it must not read.
        if samples.is_empty() {
            return Self { output_ptr: 0, size: 0 };
        }

        Self {
            output_ptr: samples.as_ptr() as u64,
            size: (samples.len() * size_of::<i16>()) as u32,
        }
    }
}

/// A SoX resampler living in the [`FFI_SERVER`] handle map.
///
/// Holds nothing but its handle id, so it is trivially `Send + Sync` even though
/// [`resampler::SoxResampler`] is `Send` and not `Sync`; the `Mutex` in
/// [`Self::Inner`] is what makes the value storable and shareable.
#[derive(uniffi::Object)]
pub struct SoxResampler {
    handle_id: FfiHandleId,
}

impl BackedByFfiHandle for SoxResampler {
    /// Identical to the type `on_push_sox_resampler` / `on_flush_sox_resampler`
    /// look up, which is what makes the two surfaces share one resampler.
    type Inner = Arc<Mutex<resampler::SoxResampler>>;

    fn from_ffi_handle_id(ffi_handle_id: FfiHandleId) -> Self {
        Self { handle_id: ffi_handle_id }
    }

    fn ffi_handle_id(&self) -> FfiHandleId {
        self.handle_id
    }
}

impl Drop for SoxResampler {
    fn drop(&mut self) {
        FFI_SERVER.drop_handle(self.handle_id);
    }
}

#[uniffi::export]
impl SoxResampler {
    /// Create a resampler and store it in the handle map.
    ///
    /// `flags` is passed through to libsoxr's quality spec unchanged, matching
    /// `NewSoxResamplerRequest.flags`.
    #[uniffi::constructor(default(flags = 0))]
    pub fn new(
        input_rate: f64,
        output_rate: f64,
        num_channels: u32,
        input_data_type: SoxResamplerDataType,
        output_data_type: SoxResamplerDataType,
        quality_recipe: SoxQualityRecipe,
        flags: u32,
    ) -> Result<Self, SoxResamplerError> {
        let io_spec = resampler::IOSpec {
            input_type: input_data_type.into(),
            output_type: output_data_type.into(),
        };
        let quality_spec = resampler::QualitySpec { quality: quality_recipe.into(), flags };
        let runtime_spec = resampler::RuntimeSpec { num_threads: 1 };

        let inner = resampler::SoxResampler::new(
            input_rate,
            output_rate,
            num_channels,
            io_spec,
            quality_spec,
            runtime_spec,
        )
        .map_err(|reason| SoxResamplerError::Create { reason })?;

        Ok(Self::from_inner(Arc::new(Mutex::new(inner))))
    }

    /// Adopt a resampler that already exists in the handle map — for instance
    /// one created through the legacy `NewSoxResamplerRequest`.
    ///
    /// This *takes ownership*: dropping the returned object drops the handle, so
    /// the previous owner must not also free it.
    #[uniffi::constructor]
    pub fn from_handle(handle_id: u64) -> Self {
        Self::from_ffi_handle_id(handle_id)
    }

    /// The handle id addressing this resampler, for driving it over the legacy C
    /// ABI.
    ///
    /// Ownership is **not** transferred: this object remains the owner, so the
    /// caller must not pass the id to `livekit_ffi_drop_handle` while this object
    /// is still alive.
    pub fn handle_id(&self) -> u64 {
        self.ffi_handle_id()
    }

    /// Resample `size` bytes of interleaved `i16` samples at `data_ptr`.
    ///
    /// # Safety
    ///
    /// `data_ptr` must point to `size` readable bytes of `i16` samples, aligned
    /// for `i16`, for the duration of the call. See [`SoxResamplerOutput`] for
    /// the returned pointer's lifetime.
    pub fn push(&self, data_ptr: u64, size: u32) -> Result<SoxResamplerOutput, SoxResamplerError> {
        let data = unsafe {
            slice::from_raw_parts(data_ptr as *const i16, size as usize / size_of::<i16>())
        };

        let resampler = self.inner()?;
        let mut resampler = resampler.lock();
        let output =
            resampler.push(data).map_err(|reason| SoxResamplerError::Process { reason })?;

        Ok(SoxResamplerOutput::from_samples(output))
    }

    /// Drain any samples libsoxr is still holding and reset the resampler.
    pub fn flush(&self) -> Result<SoxResamplerOutput, SoxResamplerError> {
        let resampler = self.inner()?;
        let mut resampler = resampler.lock();
        let output = resampler.flush().map_err(|reason| SoxResamplerError::Process { reason })?;

        Ok(SoxResamplerOutput::from_samples(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::requests;

    const INPUT_RATE: f64 = 48000.0;
    const OUTPUT_RATE: f64 = 16000.0;
    const CHUNK: usize = 4800;

    /// Serializes these tests against each other.
    ///
    /// The reason is not in this module: libsoxr as vendored here is not safe to
    /// drive from two threads at once, even with two entirely separate
    /// resamplers. `soxr-sys/build.rs` compiles it without `_OPENMP`, which
    /// turns every `ccrw2_*` lock in `soxr-sys/src/ccrw2.h` into a no-op, and
    /// the process-global FFT cache in `soxr-sys/src/fft4g_cache.h`
    /// (`LSX_FFT_BR` / `LSX_FFT_SC` / `FFT_LEN`, `realloc`'d in place) is then
    /// unsynchronized. Concurrent first-time use segfaults.
    ///
    /// The default test harness runs these in parallel, and they are the first
    /// tests in the crate to touch libsoxr at all, so without this they crash
    /// the whole binary intermittently.
    static SOXR_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn tone(len: usize) -> Vec<i16> {
        (0..len).map(|i| ((i as f64 * 0.05).sin() * 8000.0) as i16).collect()
    }

    fn new_uniffi_resampler() -> SoxResampler {
        SoxResampler::new(
            INPUT_RATE,
            OUTPUT_RATE,
            1,
            SoxResamplerDataType::Int16Interleaved,
            SoxResamplerDataType::Int16Interleaved,
            SoxQualityRecipe::Medium,
            0,
        )
        .expect("failed to create resampler over uniffi")
    }

    /// Drive `NewSoxResamplerRequest` through the legacy protobuf path and return
    /// the owned handle id it hands back to the client.
    fn legacy_new() -> FfiHandleId {
        let res = requests::handle_request(
            &FFI_SERVER,
            proto::FfiRequest {
                message: Some(proto::ffi_request::Message::NewSoxResampler(
                    proto::NewSoxResamplerRequest {
                        input_rate: INPUT_RATE,
                        output_rate: OUTPUT_RATE,
                        num_channels: 1,
                        input_data_type: proto::SoxResamplerDataType::SoxrDatatypeInt16i as i32,
                        output_data_type: proto::SoxResamplerDataType::SoxrDatatypeInt16i as i32,
                        quality_recipe: proto::SoxQualityRecipe::SoxrQualityMedium as i32,
                        flags: None,
                    },
                )),
            },
        )
        .expect("handle_request failed");

        match res.message {
            Some(proto::ffi_response::Message::NewSoxResampler(res)) => match res.message {
                Some(proto::new_sox_resampler_response::Message::Resampler(owned)) => {
                    owned.handle.id
                }
                other => panic!("legacy new failed: {other:?}"),
            },
            other => panic!("unexpected response: {other:?}"),
        }
    }

    /// Drive `PushSoxResamplerRequest` through the legacy protobuf path and
    /// return the number of output samples.
    fn legacy_push(handle_id: FfiHandleId, data: &[i16]) -> usize {
        let res = requests::handle_request(
            &FFI_SERVER,
            proto::FfiRequest {
                message: Some(proto::ffi_request::Message::PushSoxResampler(
                    proto::PushSoxResamplerRequest {
                        resampler_handle: handle_id,
                        data_ptr: data.as_ptr() as u64,
                        size: (data.len() * size_of::<i16>()) as u32,
                    },
                )),
            },
        )
        .expect("handle_request failed");

        match res.message {
            Some(proto::ffi_response::Message::PushSoxResampler(res)) => {
                assert!(res.error.is_none(), "legacy push failed: {:?}", res.error);
                res.size as usize / size_of::<i16>()
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    fn uniffi_push(resampler: &SoxResampler, data: &[i16]) -> usize {
        let out = resampler
            .push(data.as_ptr() as u64, (data.len() * size_of::<i16>()) as u32)
            .expect("uniffi push failed");
        out.size as usize / size_of::<i16>()
    }

    /// Output counts for the first and second push of an otherwise untouched
    /// resampler, measured entirely over UniFFI.
    ///
    /// libsoxr primes its filter on the first call, so these two differ — which
    /// is what makes them usable as a fingerprint for "how far along is this
    /// stream?" without hardcoding libsoxr's internals.
    fn push_fingerprint() -> (usize, usize) {
        let resampler = new_uniffi_resampler();
        let data = tone(CHUNK);

        let first = uniffi_push(&resampler, &data);
        let second = uniffi_push(&resampler, &data);

        assert_ne!(
            first, second,
            "the first and second push produce the same count, so these tests cannot tell a \
             primed resampler from a fresh one"
        );
        (first, second)
    }

    /// A resampler created over UniFFI is addressable by the legacy C ABI.
    ///
    /// Reaching the handle at all is most of the claim — a handle the C ABI could
    /// not see would fail `retrieve_handle` with "handle not found". The counts go
    /// further: the C ABI's push behaves as the *first* push of this stream, and
    /// the UniFFI push that follows behaves as the *second*, so both calls landed
    /// on one resampler rather than two.
    #[test]
    fn uniffi_created_resampler_is_reachable_from_the_c_abi() {
        let _serialized = SOXR_TEST_LOCK.lock();

        let (first, second) = push_fingerprint();

        let resampler = new_uniffi_resampler();
        let data = tone(CHUNK);

        assert_eq!(legacy_push(resampler.handle_id(), &data), first);
        assert_eq!(uniffi_push(&resampler, &data), second);
    }

    /// The reverse direction: adopt a handle the legacy path created and drive it
    /// over UniFFI.
    #[test]
    fn c_abi_created_resampler_is_reachable_from_uniffi() {
        let _serialized = SOXR_TEST_LOCK.lock();

        let (first, second) = push_fingerprint();

        let resampler = SoxResampler::from_handle(legacy_new());
        let data = tone(CHUNK);

        assert_eq!(uniffi_push(&resampler, &data), first);
        assert_eq!(legacy_push(resampler.handle_id(), &data), second);
    }

    /// The real claim: alternating calls across the two surfaces advance *one*
    /// resampler stream. Two independent resamplers would each pay the filter
    /// delay separately and the totals would not add up.
    #[test]
    fn interleaved_calls_advance_one_shared_stream() {
        let _serialized = SOXR_TEST_LOCK.lock();

        let resampler = new_uniffi_resampler();
        let handle_id = resampler.handle_id();
        let data = tone(CHUNK);

        let mut produced = 0;
        for round in 0..4 {
            produced += if round % 2 == 0 {
                uniffi_push(&resampler, &data)
            } else {
                legacy_push(handle_id, &data)
            };
        }

        let flushed = resampler.flush().expect("uniffi flush failed");
        produced += flushed.size as usize / size_of::<i16>();

        // 4 * 4800 samples at 48k -> 16k is ~6400 out; only the initial filter
        // delay should be missing, and only once.
        let expected = 4 * CHUNK / 3;
        assert!(
            produced > expected - 200 && produced <= expected + 200,
            "interleaved pushes produced {produced} samples in total, expected ~{expected}"
        );
    }

    /// Ownership is RAII: the handle goes away with the object.
    #[test]
    fn drop_frees_the_handle() {
        let _serialized = SOXR_TEST_LOCK.lock();

        let handle_id = {
            let resampler = new_uniffi_resampler();
            resampler.handle_id()
        };

        assert!(
            FFI_SERVER.retrieve_handle::<Arc<Mutex<resampler::SoxResampler>>>(handle_id).is_err(),
            "handle {handle_id} outlived the uniffi object that owned it"
        );
    }

    /// ...unless ownership is explicitly handed back out, which is how a caller
    /// moves a resampler to the legacy C ABI.
    #[test]
    fn leak_ffi_handle_id_keeps_the_handle_alive() {
        let _serialized = SOXR_TEST_LOCK.lock();

        let handle_id = {
            let resampler = new_uniffi_resampler();
            resampler.leak_ffi_handle_id()
        };

        assert!(
            FFI_SERVER.retrieve_handle::<Arc<Mutex<resampler::SoxResampler>>>(handle_id).is_ok(),
            "leaked handle {handle_id} was dropped anyway"
        );

        // We now own it, exactly as a client of the C ABI would.
        assert!(FFI_SERVER.drop_handle(handle_id));
    }

    /// A handle of the wrong type is a downcast failure in `retrieve_handle`, and
    /// must surface as an error rather than a panic.
    #[test]
    fn adopting_a_mistyped_handle_reports_invalid_handle() {
        let _serialized = SOXR_TEST_LOCK.lock();

        let handle_id = FFI_SERVER.next_id();
        FFI_SERVER.store_handle(handle_id, ());

        let resampler = SoxResampler::from_handle(handle_id);
        let data = tone(64);

        let err = resampler
            .push(data.as_ptr() as u64, (data.len() * size_of::<i16>()) as u32)
            .expect_err("push on a mistyped handle should fail");

        assert!(
            matches!(err, SoxResamplerError::InvalidHandle { .. }),
            "expected InvalidHandle, got {err:?}"
        );
    }
}
