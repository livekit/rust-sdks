//
// Created by Théo Monnom on 30/05/2022.
//

#ifndef LIVEKIT_NATIVE_PEER_TRANSPORT_H
#define LIVEKIT_NATIVE_PEER_TRANSPORT_H

#include <api/peer_connection_interface.h>
#include "peer_observer.h"

namespace livekit {

    class RTCEngine;

    class PeerTransport {
    public:
        explicit PeerTransport(const RTCEngine &rtc_engine);
        void Negotiate();

    private:
        rtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection;
        std::unique_ptr<PeerObserver> observer;
    };

} // livekit

#endif //LIVEKIT_NATIVE_PEER_TRANSPORT_H
