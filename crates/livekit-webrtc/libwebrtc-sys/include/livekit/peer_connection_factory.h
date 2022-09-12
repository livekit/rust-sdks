//
// Created by Théo Monnom on 03/08/2022.
//


#ifndef PEER_CONNECTION_FACTORY_H
#define PEER_CONNECTION_FACTORY_H

#include "api/peer_connection_interface.h"
#include "peer_connection.h"
#include "rust_types.h"

namespace livekit {
    using NativeRTCConfiguration = webrtc::PeerConnectionInterface::RTCConfiguration;

    class PeerConnectionFactory {
    public:
        PeerConnectionFactory();

        std::unique_ptr<PeerConnection> create_peer_connection(std::unique_ptr<NativeRTCConfiguration> config, std::unique_ptr<NativePeerConnectionObserver> observer) const;

    private:
        std::unique_ptr<rtc::Thread> network_thread_;
        std::unique_ptr<rtc::Thread> worker_thread_;
        std::unique_ptr<rtc::Thread> signaling_thread_;

        rtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> peer_factory_;
    };

    std::unique_ptr<PeerConnectionFactory> create_peer_connection_factory();
    std::unique_ptr<NativeRTCConfiguration> create_rtc_configuration(RTCConfiguration conf);
} // livekit


#endif //PEER_CONNECTION_FACTORY_H
