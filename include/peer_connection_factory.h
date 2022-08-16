//
// Created by Théo Monnom on 03/08/2022.
//


#ifndef PEER_CONNECTION_FACTORY_H
#define PEER_CONNECTION_FACTORY_H

#include "api/peer_connection_interface.h"

namespace lk {

    class PeerConnectionFactory {
    public:
        PeerConnectionFactory();

    private:
        rtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> peer_factory_;

        std::unique_ptr<rtc::Thread> network_thread_;
        std::unique_ptr<rtc::Thread> worker_thread_;
        std::unique_ptr<rtc::Thread> signaling_thread_;
    };

    std::unique_ptr<PeerConnectionFactory> CreatePeerConnectionFactory();

}


#endif //PEER_CONNECTION_FACTORY_H
