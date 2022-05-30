//
// Created by Théo Monnom on 30/05/2022.
//

#ifndef LIVEKIT_NATIVE_CREATESESSION_OBSERVER_H
#define LIVEKIT_NATIVE_CREATESESSION_OBSERVER_H

#include <api/peer_connection_interface.h>

namespace livekit {

    class CreateSessionObserver : public webrtc::CreateSessionDescriptionObserver {
    public:

        void OnSuccess(webrtc::SessionDescriptionInterface *desc) override {
            
        };

        void OnFailure(webrtc::RTCError error) override {

        };
    };

} // livekit

#endif //LIVEKIT_NATIVE_CREATESESSION_OBSERVER_H
