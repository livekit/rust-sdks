//
// Created by Théo Monnom on 01/09/2022.
//

#include "livekit/candidate.h"

namespace livekit {
    Candidate::Candidate(const cricket::Candidate &candidate) : candidate_(candidate) {

    }
} // livekit