//
// Created by Théo Monnom on 21/05/2022.
//

#ifndef LIVEKIT_NATIVE_PEER_OBSERVER_H
#define LIVEKIT_NATIVE_PEER_OBSERVER_H

#include <api/create_peerconnection_factory.h>
#include <spdlog/spdlog.h>

namespace livekit {

    class PeerObserver : public webrtc::PeerConnectionObserver {
    public:

        void OnSignalingChange(webrtc::PeerConnectionInterface::SignalingState new_state) override {
            spdlog::info("Received OnSignalingChange");
        };

        void OnAddStream(rtc::scoped_refptr<webrtc::MediaStreamInterface> stream) override {
            spdlog::info("Received OnAddStream");
        };

        void OnRemoveStream(rtc::scoped_refptr<webrtc::MediaStreamInterface> stream) override {
            spdlog::info("Received OnRemoveStream");
        };

        void OnDataChannel(rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) override {
            spdlog::info("Received OnDataChannel");
        };

        void OnRenegotiationNeeded() override {
            spdlog::info("Received OnRenegotiationNeeded");
        };

        void OnIceConnectionChange(webrtc::PeerConnectionInterface::IceConnectionState new_state) override {
            spdlog::info("Received OnIceConnectionChange");
        };

        void OnIceGatheringChange(webrtc::PeerConnectionInterface::IceGatheringState new_state) override {
            spdlog::info("Received OnIceGatheringChange");
        };

        void OnIceCandidate(const webrtc::IceCandidateInterface *candidate) override {
            spdlog::info("Received OnIceCandidate");
        };
    };

} // livekit

#endif //LIVEKIT_NATIVE_PEER_OBSERVER_H
