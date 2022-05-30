//
// Created by Théo Monnom on 04/05/2022.
//

#ifndef LIVEKIT_NATIVE_RTC_ENGINE_H
#define LIVEKIT_NATIVE_RTC_ENGINE_H

#include "signal_client.h"
#include "peer_observer.h"
#include "peer_transport.h"
#include <api/peer_connection_interface.h>

namespace livekit{

    class RTCEngine {

    public:
        RTCEngine();

        void Join(const std::string &url, const std::string &token);
        void Update();

    private:
        void Configure();
        void OnJoin(const JoinResponse &res);

    private:
        friend class PeerTransport;

        SignalClient client_;
        
        rtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> peer_factory_;
        webrtc::PeerConnectionInterface::RTCConfiguration configuration_;
        std::unique_ptr<rtc::Thread> network_thread_;
        std::unique_ptr<rtc::Thread> worker_thread_;
        std::unique_ptr<rtc::Thread> signaling_thread_;

        std::unique_ptr<PeerTransport> publisher_;
        std::unique_ptr<PeerTransport> subscriber_;
    };
} // livekit

#endif //LIVEKIT_NATIVE_RTC_ENGINE_H
