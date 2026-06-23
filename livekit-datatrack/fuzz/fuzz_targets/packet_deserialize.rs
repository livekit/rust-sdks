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

#![no_main]

use bytes::Bytes;
use fake::Fake;
use libfuzzer_sys::fuzz_target;
use livekit_datatrack::__fuzz::Packet;
use std::{fs, io::Write, path::PathBuf, sync::Once};

const SEED_COUNT: usize = 16;
const SEED_MAX_PAYLOAD_LEN: usize = 64;

fn generate_seed_packets() {
    let corpus =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus").join("packet_deserialize");
    let Ok(()) = fs::create_dir_all(&corpus) else { return };

    for i in 0..SEED_COUNT {
        let path = corpus.join(format!("seed_{i}"));
        let Ok(mut f) = fs::OpenOptions::new().write(true).create_new(true).open(&path) else {
            // Seed for this index already exists.
            continue;
        };
        let packet: Packet = SEED_MAX_PAYLOAD_LEN.fake();
        let _ = f.write_all(&packet.serialize());
    }
}

fuzz_target!(|data: &[u8]| {
    static SEED_INIT: Once = Once::new();
    SEED_INIT.call_once(generate_seed_packets);
    let _ = Packet::deserialize(Bytes::copy_from_slice(data));
});
