//
// Created by Théo Monnom on 01/09/2022.
//

#ifndef CLIENT_SDK_NATIVE_CANDIDATE_H
#define CLIENT_SDK_NATIVE_CANDIDATE_H

#include <memory>
#include "api/candidate.h"

// cricket::Candidate
namespace livekit {

    class Candidate {
    public:
        explicit Candidate(const cricket::Candidate &candidate);

    private:
        cricket::Candidate candidate_;
    };

    static std::unique_ptr<Candidate> _unique_candidate(){
        return nullptr;
    }

} // livekit

#endif //CLIENT_SDK_NATIVE_CANDIDATE_H
