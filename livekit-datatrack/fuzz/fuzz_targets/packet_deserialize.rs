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
