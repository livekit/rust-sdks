#include "peer.h"

#include <memory>
#include <string>

#include "rtc_base/logging.h"
#include "system_wrappers/include/sleep.h"
#include "test/gmock.h"
#include "test/gtest.h"

namespace livekit_ffi {

void onSuccessCb0(void* userdata) {
  RTC_LOG(LS_INFO) << "SetDescription onSuccess called";
}
void onFailureCb(const lkRtcError* error, void* userdata) {
  RTC_LOG_ERR(LS_ERROR) << "CreateOffer onFailure called: " << error->message;
}

void onSuccessCb(lkSdpType type, const char* sdp, void* userdata) {
  RTC_LOG(LS_INFO) << "CreateOffer onSuccess called" << ", type: " << type
                   << ", sdp: " << sdp;

  Peer* peer = reinterpret_cast<Peer*>(userdata);
  // For testing, we just set the remote description to be the same as local
  const lkSetSdpObserver observer = {
      .onSuccess = onSuccessCb0,
      .onFailure = onFailureCb,
  };

  peer->SetLocalDescription(type, sdp, &observer, userdata);
}

void onIceCandidateCb(const lkIceCandidate* cand, void* _) {
  RTC_LOG(LS_INFO) << "onIceCandidate called: " << cand->sdp;
}

void onSignalingChangeCb(lkSignalingState new_state, void* userdata) {
  RTC_LOG(LS_INFO) << "onSignalingChange called: " << new_state;
}

void onTrackCb(const lkRtpTransceiver* transceiver, void* userdata) {
  RTC_LOG(LS_INFO) << "onTrack called " << transceiver;
}

TEST(LIVEKIT_RTC, ConstructDestruct) {
  RTC_LOG(LS_INFO) << "PeerFactory() called";
  auto peer_factory = webrtc::make_ref_counted<livekit_ffi::PeerFactory>();
  EXPECT_NE(peer_factory, nullptr);

  const lkPeerObserver callbacks = {
      .onSignalingChange = onSignalingChangeCb,
      .onIceCandidate = onIceCandidateCb,
      .onDataChannel = nullptr,
      .onTrack = onTrackCb,
      .onConnectionChange = nullptr,
      .onIceCandidateError = nullptr,
  };

  void* userdata = nullptr;
  lkIceServer iceServer0 = {
          .urls = new const char*[1]{ "stun:stun.l.google.com:19302" },
          .urlsCount = 1,
          .username = "",
          .password = "",
      };
  const lkRtcConfiguration config = {
      .iceServers = &iceServer0,
      .iceServersCount = 1,
      .iceTransportType = lkIceTransportType::LK_ICE_TRANSPORT_TYPE_ALL,
      .gatheringPolicy = lkContinualGatheringPolicy::LK_GATHERING_POLICY_ONCE,
  };

  auto peer = peer_factory->CreatePeer(&config, &callbacks, userdata);

  EXPECT_NE(peer, nullptr);

  const lkCreateSdpObserver createSdpObserver = {
      .onSuccess = onSuccessCb,
      .onFailure = onFailureCb,
  };

  peer->CreateOffer(
      lkOfferAnswerOptions{
          .iceRestart = false,
          .useRtpMux = true,
      },
      &createSdpObserver, peer.get());

  // wait for async operations to complete
  webrtc::SleepMs(4000);

  RTC_LOG(LS_INFO) << "PeerFactory() destroyed";
}

}  // namespace livekit_ffi
