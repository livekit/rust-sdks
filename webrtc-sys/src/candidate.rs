#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/candidate.h");

        type Candidate; // cricket::Candidate

        // TODO SHARED
    }
}
