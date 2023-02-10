#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    // Wrapper to opaque C++ objects
    // https://github.com/dtolnay/cxx/issues/741
    // Use SharedPtr/UniquePtr inside Vec
    pub struct MediaStreamPtr {
        pub ptr: SharedPtr<MediaStream>,
    }

    pub struct CandidatePtr {
        pub ptr: SharedPtr<Candidate>,
    }

    extern "C++" {
        include!("livekit/media_stream.h");
        include!("livekit/candidate.h");

        type MediaStream = crate::media_stream::ffi::MediaStream;
        type Candidate = crate::candidate::ffi::Candidate;
    }
}
