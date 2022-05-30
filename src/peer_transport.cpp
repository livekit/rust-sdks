//
// Created by Théo Monnom on 30/05/2022.
//

#include "peer_transport.h"
#include "rtc_engine.h"

namespace livekit {
    PeerTransport::PeerTransport(const RTCEngine &rtc_engine) {
        observer = std::make_unique<PeerObserver>();
        webrtc::PeerConnectionDependencies peer_configuration{observer.get()};

        webrtc::RTCErrorOr<rtc::scoped_refptr<webrtc::PeerConnectionInterface>> opt_peer = rtc_engine.peer_factory_->CreatePeerConnectionOrError(
                rtc_engine.configuration_, std::move(peer_configuration));

        if (!opt_peer.ok()) {
            throw std::runtime_error{"Failed to create a peer connection"};
        }

        peer_connection = opt_peer.value();
    }

    void PeerTransport::Negotiate() {
    }

} // livekit