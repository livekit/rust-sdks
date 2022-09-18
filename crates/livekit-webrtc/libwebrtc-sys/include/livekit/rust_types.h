//
// Created by Théo Monnom on 30/08/2022.
//


#ifndef RUST_TYPES_H
#define RUST_TYPES_H

#include "api/peer_connection_interface.h"

namespace livekit {
    struct RTCConfiguration;
    struct PeerConnectionObserverWrapper;
    struct CreateSdpObserverWrapper;
    struct SetLocalSdpObserverWrapper;
    struct SetRemoteSdpObserverWrapper;
    struct DataChannelObserverWrapper;
    struct AddIceCandidateObserverWrapper;

    // Shared types
    struct RTCOfferAnswerOptions;
    struct RTCError;
    struct DataChannelInit;
}

#endif //RUST_TYPES_H
