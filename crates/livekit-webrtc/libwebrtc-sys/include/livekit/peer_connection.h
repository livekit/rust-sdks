//
// Created by Théo Monnom on 30/08/2022.
//

#ifndef CLIENT_SDK_NATIVE_PEER_CONNECTION_H
#define CLIENT_SDK_NATIVE_PEER_CONNECTION_H

#include <memory>

#include "api/peer_connection_interface.h"
#include "data_channel.h"
#include "jsep.h"
#include "rust/cxx.h"
#include "rust_types.h"

namespace livekit {
class NativeAddIceCandidateObserver;

class PeerConnection {
 public:
  explicit PeerConnection(
      rtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection);

  void create_offer(NativeCreateSdpObserverHandle& observer,
                    RTCOfferAnswerOptions options);
  void create_answer(NativeCreateSdpObserverHandle& observer,
                     RTCOfferAnswerOptions options);
  void set_local_description(std::unique_ptr<SessionDescription> desc,
                             NativeSetLocalSdpObserverHandle& observer);
  void set_remote_description(std::unique_ptr<SessionDescription> desc,
                              NativeSetRemoteSdpObserverHandle& observer);
  std::unique_ptr<DataChannel> create_data_channel(
      rust::String label,
      std::unique_ptr<NativeDataChannelInit> init);
  void add_ice_candidate(std::unique_ptr<IceCandidate> candidate,
                         NativeAddIceCandidateObserver& observer);
  void close();

 private:
  rtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection_;
};

static std::unique_ptr<PeerConnection> _unique_peer_connection() {
  return nullptr;  // Ignore
}

class NativeAddIceCandidateObserver {
 public:
  explicit NativeAddIceCandidateObserver(
      rust::Box<AddIceCandidateObserverWrapper> observer);

  void OnComplete(const RTCError& error);

 private:
  rust::Box<AddIceCandidateObserverWrapper> observer_;
};

std::unique_ptr<NativeAddIceCandidateObserver>
create_native_add_ice_candidate_observer(
    rust::Box<AddIceCandidateObserverWrapper> observer);

class NativePeerConnectionObserver : public webrtc::PeerConnectionObserver {
 public:
  explicit NativePeerConnectionObserver(
      rust::Box<PeerConnectionObserverWrapper> observer);

  void OnSignalingChange(
      webrtc::PeerConnectionInterface::SignalingState new_state) override;

  void OnAddStream(
      rtc::scoped_refptr<webrtc::MediaStreamInterface> stream) override;

  void OnRemoveStream(
      rtc::scoped_refptr<webrtc::MediaStreamInterface> stream) override;

  void OnDataChannel(
      rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) override;

  void OnRenegotiationNeeded() override;

  void OnNegotiationNeededEvent(uint32_t event_id) override;

  void OnIceConnectionChange(
      webrtc::PeerConnectionInterface::IceConnectionState new_state) override;

  void OnStandardizedIceConnectionChange(
      webrtc::PeerConnectionInterface::IceConnectionState new_state) override;

  void OnConnectionChange(
      webrtc::PeerConnectionInterface::PeerConnectionState new_state) override;

  void OnIceGatheringChange(
      webrtc::PeerConnectionInterface::IceGatheringState new_state) override;

  void OnIceCandidate(const webrtc::IceCandidateInterface* candidate) override;

  void OnIceCandidateError(const std::string& address,
                           int port,
                           const std::string& url,
                           int error_code,
                           const std::string& error_text) override;

  void OnIceCandidatesRemoved(
      const std::vector<cricket::Candidate>& candidates) override;

  void OnIceConnectionReceivingChange(bool receiving) override;

  void OnIceSelectedCandidatePairChanged(
      const cricket::CandidatePairChangeEvent& event) override;

  void OnAddTrack(
      rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver,
      const std::vector<rtc::scoped_refptr<webrtc::MediaStreamInterface>>&
          streams) override;

  void OnTrack(
      rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) override;

  void OnRemoveTrack(
      rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver) override;

  void OnInterestingUsage(int usage_pattern) override;

 private:
  rust::Box<PeerConnectionObserverWrapper> observer_;
};

std::unique_ptr<NativePeerConnectionObserver>
create_native_peer_connection_observer(
    rust::Box<PeerConnectionObserverWrapper> observer);
}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_PEER_CONNECTION_H
