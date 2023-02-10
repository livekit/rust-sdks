///
/// This file should not be included in other headers
///

#ifndef RUST_HELPER_H
#define RUST_HELPER_H

#include "rust/cxx.h"

namespace livekit {

struct MediaStreamPtr;
struct CandidatePtr;

// Impl not needed
static rust::Vec<MediaStreamPtr> _vec_media_stream_ptr() {
  throw;
}
static rust::Vec<CandidatePtr> _vec_candidate_ptr() {
  throw;
}

}  // namespace livekit

#endif  // RUST_HELPER_H
