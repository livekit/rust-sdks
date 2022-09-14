//
// Created by Théo Monnom on 01/09/2022.
//

#ifndef CLIENT_SDK_NATIVE_JSEP_H
#define CLIENT_SDK_NATIVE_JSEP_H

#include <memory>
#include "api/jsep.h"
#include "rust/cxx.h"
#include "rust_types.h"
#include "api/ref_counted_base.h"

namespace livekit {

    class IceCandidate {
    public:
        explicit IceCandidate(std::unique_ptr<webrtc::IceCandidateInterface> ice_candidate);
    private:
        std::unique_ptr<webrtc::IceCandidateInterface> ice_candidate_;
    };

    static std::unique_ptr<IceCandidate> _unique_ice_candidate(){
        return nullptr; // Ignore
    }

    class SessionDescription {
    public:
        explicit SessionDescription(std::unique_ptr<webrtc::SessionDescriptionInterface> session_description);

        rust::String stringify() const;
        std::unique_ptr<SessionDescription> clone() const;
        std::unique_ptr<webrtc::SessionDescriptionInterface> release();

    private:
        std::unique_ptr<webrtc::SessionDescriptionInterface> session_description_;
    };

    static std::unique_ptr<SessionDescription> _unique_session_description(){
        return nullptr; // Ignore
    }

    static std::shared_ptr<SessionDescription> _shared_session_description(){
        return nullptr; // Ignore
    }

    // SetCreateSdpObserver

    class NativeCreateSdpObserver : public webrtc::CreateSessionDescriptionObserver {
    public:
        explicit NativeCreateSdpObserver(rust::Box<CreateSdpObserverWrapper> observer);

        void OnSuccess(webrtc::SessionDescriptionInterface* desc) override;
        void OnFailure(webrtc::RTCError error) override;
    private:
        rust::Box<CreateSdpObserverWrapper> observer_;
    };

    struct NativeCreateSdpObserverHandle {
        rtc::scoped_refptr<NativeCreateSdpObserver> observer;
    };

    std::unique_ptr<NativeCreateSdpObserverHandle> create_native_create_sdp_observer(rust::Box<CreateSdpObserverWrapper> observer);

    // SetLocalSdpObserver

    class NativeSetLocalSdpObserver : public webrtc::SetLocalDescriptionObserverInterface{
    public:
        explicit NativeSetLocalSdpObserver(rust::Box<SetLocalSdpObserverWrapper> observer);

        void OnSetLocalDescriptionComplete(webrtc::RTCError error) override;
    private:
        rust::Box<SetLocalSdpObserverWrapper> observer_;
    };

    struct NativeSetLocalSdpObserverHandle {
        rtc::scoped_refptr<NativeSetLocalSdpObserver> observer;
    };

    std::unique_ptr<NativeSetLocalSdpObserverHandle> create_native_set_local_sdp_observer(rust::Box<SetLocalSdpObserverWrapper> observer);

    // SetRemoteSdpObserver

    class NativeSetRemoteSdpObserver : public webrtc::SetRemoteDescriptionObserverInterface{
    public:
        explicit NativeSetRemoteSdpObserver(rust::Box<SetRemoteSdpObserverWrapper> observer);

        void OnSetRemoteDescriptionComplete(webrtc::RTCError error) override;
    private:
        rust::Box<SetRemoteSdpObserverWrapper> observer_;
    };

    struct NativeSetRemoteSdpObserverHandle {
        rtc::scoped_refptr<NativeSetRemoteSdpObserver> observer;
    };

    std::unique_ptr<NativeSetRemoteSdpObserverHandle> create_native_set_remote_sdp_observer(rust::Box<SetRemoteSdpObserverWrapper> observer);
} // livekit

#endif //CLIENT_SDK_NATIVE_JSEP_H
