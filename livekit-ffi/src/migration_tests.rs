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

//! Protobuf-driven tests for the types being migrated to uniffi.
//!
//! The FFI request surface is the contract that must not move while the types
//! behind it grow uniffi annotations, so these tests drive it the way the
//! bindings do — encoded requests in, handles and raw pointers out — and check
//! the result against the same work done through the Rust API directly. The
//! direct-call half of each pair lives next to the code it exercises, e.g.
//! `server::resampler::migration_tests`.
//!
//! They are written to compile and pass either side of the migration, so they
//! can be replayed on the commit before it.

use std::{mem::size_of, slice};

use crate::{proto, server::requests::handle_request, FfiHandleId, FFI_SERVER};

fn request(message: impl Into<proto::ffi_request::Message>) -> proto::ffi_response::Message {
    let response = handle_request(&FFI_SERVER, proto::FfiRequest { message: Some(message.into()) })
        .expect("request failed");
    response.message.expect("response carried no message")
}

fn new_sox_resampler(
    input_rate: f64,
    output_rate: f64,
    num_channels: u32,
    quality: proto::SoxQualityRecipe,
) -> FfiHandleId {
    let response = match request(proto::NewSoxResamplerRequest {
        input_rate,
        output_rate,
        num_channels,
        input_data_type: proto::SoxResamplerDataType::SoxrDatatypeInt16i as i32,
        output_data_type: proto::SoxResamplerDataType::SoxrDatatypeInt16i as i32,
        quality_recipe: quality as i32,
        flags: None,
    }) {
        proto::ffi_response::Message::NewSoxResampler(response) => response,
        _ => panic!("expected a NewSoxResampler response"),
    };

    match response.message.expect("response carried no message") {
        proto::new_sox_resampler_response::Message::Resampler(resampler) => resampler.handle.id,
        proto::new_sox_resampler_response::Message::Error(error) => panic!("{error}"),
    }
}

fn push_sox_resampler(handle: FfiHandleId, input: &[i16]) -> Vec<i16> {
    let response = match request(proto::PushSoxResamplerRequest {
        resampler_handle: handle,
        data_ptr: input.as_ptr() as u64,
        size: (input.len() * size_of::<i16>()) as u32,
    }) {
        proto::ffi_response::Message::PushSoxResampler(response) => response,
        _ => panic!("expected a PushSoxResampler response"),
    };

    assert_eq!(response.error, None, "push failed");
    read_output(response.output_ptr, response.size)
}

fn flush_sox_resampler(handle: FfiHandleId) -> Vec<i16> {
    let response = match request(proto::FlushSoxResamplerRequest { resampler_handle: handle }) {
        proto::ffi_response::Message::FlushSoxResampler(response) => response,
        _ => panic!("expected a FlushSoxResampler response"),
    };

    assert_eq!(response.error, None, "flush failed");
    read_output(response.output_ptr, response.size)
}

/// The pointer/size pair the bindings dereference: `size` counts bytes, the
/// samples are `i16`, a run that produced nothing is a null pointer, and the
/// samples stay readable until the next call on the same resampler.
fn read_output(output_ptr: u64, size: u32) -> Vec<i16> {
    assert_eq!(size as usize % size_of::<i16>(), 0, "size {size} is not whole i16 samples");
    if output_ptr == 0 || size == 0 {
        return Vec::new();
    }
    unsafe { slice::from_raw_parts(output_ptr as *const i16, size as usize / size_of::<i16>()) }
        .to_vec()
}

/// soxr dithers its int16 output, so two instances fed identical input agree to
/// within a LSB or so rather than bit for bit.
fn assert_samples_match(actual: &[i16], expected: &[i16], context: &str) {
    assert_eq!(actual.len(), expected.len(), "{context}: output length");
    for (i, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (*actual as i32 - *expected as i32).abs() <= 2,
            "{context}: sample {i} is {actual}, expected ~{expected}"
        );
    }
}

/// Every knob of `NewSoxResamplerRequest` reaches the resampler: the same input
/// pushed through the FFI request surface and through the Rust API must come
/// back as the same audio, for each rate, channel count and quality recipe.
#[test]
fn proto_push_matches_direct_push() {
    let cases = [
        (48000.0, 16000.0, 1, proto::SoxQualityRecipe::SoxrQualityQuick),
        (48000.0, 16000.0, 1, proto::SoxQualityRecipe::SoxrQualityVeryhigh),
        (16000.0, 48000.0, 1, proto::SoxQualityRecipe::SoxrQualityMedium),
        (48000.0, 24000.0, 2, proto::SoxQualityRecipe::SoxrQualityHigh),
    ];

    for (input_rate, output_rate, num_channels, quality) in cases {
        let input: Vec<i16> = (0..960 * num_channels as i16)
            .map(|i| (i % 97) * 300 - 14000) // a repeating sawtooth, loud enough to compare
            .collect();

        let handle = new_sox_resampler(input_rate, output_rate, num_channels, quality);
        #[allow(unused_mut)]
        let mut direct = crate::sox_resampler!(input_rate, output_rate, num_channels, quality);

        let context = format!("{input_rate} -> {output_rate}, {num_channels}ch, {quality:?}");
        for _ in 0..3 {
            assert_samples_match(
                &push_sox_resampler(handle, &input),
                &direct.push(&input).unwrap(),
                &format!("push at {context}"),
            );
        }
        assert_samples_match(
            &flush_sox_resampler(handle),
            &direct.flush().unwrap(),
            &format!("flush at {context}"),
        );

        FFI_SERVER.drop_handle(handle);
    }
}

/// 30ms in at 48kHz is 30ms out at 16kHz, counted in bytes off the wire.
#[test]
fn proto_push_then_flush_conserves_duration() {
    let handle = new_sox_resampler(48000.0, 16000.0, 1, proto::SoxQualityRecipe::SoxrQualityQuick);
    let frame = vec![1000i16; 480]; // 10ms @ 48kHz

    let mut frames = 0;
    for _ in 0..3 {
        frames += push_sox_resampler(handle, &frame).len();
    }
    frames += flush_sox_resampler(handle).len();

    assert!((frames as i64 - 480).abs() <= 8, "got {frames} frames, expected ~480");

    FFI_SERVER.drop_handle(handle);
}

/// The FFI handle map is a *second* owner of a migrated object, and it only
/// takes that ownership when `ffi_handle_id()` publishes an id. A resampler
/// that never crosses into the FFI path must die with its last `Arc`.
#[test]
fn the_handle_map_only_owns_published_resamplers() {
    use crate::{server::resampler::SoxResampler, sox_resampler};
    use std::sync::Arc;

    let resampler = sox_resampler!(48000.0, 16000.0, 1, proto::SoxQualityRecipe::SoxrQualityQuick);
    assert_eq!(Arc::strong_count(&resampler), 1, "an unpublished resampler has no other owner");

    let handle = resampler.clone().ffi_handle_id();
    assert_eq!(
        Arc::strong_count(&resampler),
        2,
        "publishing an id makes the FFI server a co-owner"
    );

    drop(resampler);
    assert!(
        FFI_SERVER.retrieve_handle::<Arc<SoxResampler>>(handle).is_ok(),
        "the FFI server holds the resampler until its handle is dropped"
    );

    FFI_SERVER.drop_handle(handle);
    assert!(FFI_SERVER.retrieve_handle::<Arc<SoxResampler>>(handle).is_err());
}

/// A resampler that has just been fed a loud block is still ringing: pushing
/// silence into it comes back non-silent until the filter drains. A *different*
/// resampler, fed the same silence, answers with silence — so this is the probe
/// for "the two surfaces are driving one object", and it needs a quality recipe
/// with a filter long enough to ring.
fn assert_still_ringing(output: &[i16], context: &str) {
    let peak = output.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    assert!(peak > 1000, "{context}: peak is {peak}, expected the tail of the loud block");
}

/// A resampler created over the FFI can be picked up by the uniffi side: the id
/// round-trips, and the uniffi side sees the filter state the FFI side left.
#[test]
fn a_proto_resampler_is_the_same_object_over_uniffi() {
    use crate::server::resampler::SoxResampler;

    let quality = proto::SoxQualityRecipe::SoxrQualityVeryhigh;
    let handle = new_sox_resampler(48000.0, 16000.0, 1, quality);
    let resampler = SoxResampler::from_ffi_handle_id(handle).expect("handle resolves to an object");
    assert_eq!(resampler.clone().ffi_handle_id(), handle, "the object keeps the published id");

    push_sox_resampler(handle, &vec![8000i16; 4800]); // 100ms of loud, over the FFI
    assert_still_ringing(
        &resampler.push(&vec![0i16; 4800]).unwrap(),
        "silence pushed over uniffi after a loud block over the FFI",
    );

    FFI_SERVER.drop_handle(handle);
}

/// The same trip the other way, and back again: a resampler created over uniffi
/// publishes a handle the FFI request surface drives, `take_ffi_handle_id` hands
/// sole ownership back to the uniffi side, and publishing a second time restores
/// the same id — so a migrating object can cross the seam as often as it needs to.
#[test]
fn a_resampler_hands_back_and_forth_across_the_seam() {
    use crate::{server::resampler::SoxResampler, sox_resampler};
    use std::sync::Arc;

    let loud = vec![8000i16; 4800]; // 100ms @ 48kHz
    let silence = vec![0i16; 4800];
    let resampler =
        sox_resampler!(48000.0, 16000.0, 1, proto::SoxQualityRecipe::SoxrQualityVeryhigh);

    // uniffi drives it first
    assert_still_ringing(&resampler.push(&loud).unwrap(), "push over uniffi");

    // published, the FFI side picks up the filter uniffi left behind
    let handle = resampler.clone().ffi_handle_id();
    assert_still_ringing(
        &push_sox_resampler(handle, &silence),
        "silence pushed over the FFI after a loud block over uniffi",
    );

    // the FFI side lets go, and the uniffi side carries on alone
    resampler.take_ffi_handle_id().expect("the FFI side co-owned the resampler");
    assert!(
        SoxResampler::from_ffi_handle_id(handle).is_err(),
        "a released handle is absent from the map until it is published again"
    );
    assert_still_ringing(&resampler.push(&loud).unwrap(), "push over uniffi after the take");

    // publishing again restores the handle, id and all
    assert_eq!(resampler.clone().ffi_handle_id(), handle, "republishing keeps the id");
    assert_still_ringing(
        &push_sox_resampler(handle, &silence),
        "silence pushed over the FFI after republishing",
    );

    // and the id resolves back to the very same object
    let same = SoxResampler::from_ffi_handle_id(handle).expect("the republished handle resolves");
    assert!(Arc::ptr_eq(&same, &resampler), "the handle resolves to the object that published it");
    same.push(&loud).expect("uniffi still drives it at the end of the round trip");

    FFI_SERVER.drop_handle(handle);
}
