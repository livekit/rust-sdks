//
// Created by Théo Monnom on 01/09/2022.
//

#pragma once

#include <memory>

#include "api/candidate.h"

namespace livekit {
class Candidate;
}
#include "webrtc-sys/src/candidate.rs.h"

// cricket::Candidate
namespace livekit {

class Candidate {
 public:
  explicit Candidate(const cricket::Candidate& candidate);

 private:
  cricket::Candidate candidate_;
};

static std::shared_ptr<Candidate> _shared_candidate() {
  return nullptr;
}

}  // namespace livekit
