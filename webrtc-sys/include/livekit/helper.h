#pragma once

#include "rust/cxx.h"
#include "webrtc-sys/src/helper.rs.h"

namespace livekit {

// Impl not needed
static rust::Vec<MediaStreamPtr> _vec_media_stream_ptr() {
  throw;
}
static rust::Vec<CandidatePtr> _vec_candidate_ptr() {
  throw;
}

}  // namespace livekit
