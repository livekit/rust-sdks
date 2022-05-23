//
// Created by Théo Monnom on 21/05/2022.
//

#ifndef LIVEKIT_NATIVE_PEER_OBSERVER_H
#define LIVEKIT_NATIVE_PEER_OBSERVER_H

#include <api/create_peerconnection_factory.h>

namespace livekit {

    class PeerObserver : public webrtc::PeerConnectionObserver {
    public:

     

        void OnSignalingChange(webrtc::PeerConnectionInterface::SignalingState new_state) override {

        };

        void OnAddStream(rtc::scoped_refptr<webrtc::MediaStreamInterface> stream) override {

        };

        void OnRemoveStream(rtc::scoped_refptr<webrtc::MediaStreamInterface> stream) override {

        };

        void OnDataChannel(rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) override {

        };

        void OnRenegotiationNeeded() override {

        };

        void OnIceConnectionChange(webrtc::PeerConnectionInterface::IceConnectionState new_state) override {

        };

        void OnIceGatheringChange(webrtc::PeerConnectionInterface::IceGatheringState new_state) override {

        };

        void OnIceCandidate(const webrtc::IceCandidateInterface *candidate) override {

        };
    };

} // livekit

#endif //LIVEKIT_NATIVE_PEER_OBSERVER_H
