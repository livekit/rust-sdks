//
// Created by Théo Monnom on 30/08/2022.
//

#ifndef CLIENT_SDK_NATIVE_PEER_CONNECTION_H
#define CLIENT_SDK_NATIVE_PEER_CONNECTION_H

#include <memory>

#include "api/peer_connection_interface.h"
#include "data_channel.h"
#include "jsep.h"
#include "livekit/media_stream.h"
#include "livekit/rtp_receiver.h"
#include "livekit/rtp_sender.h"
#include "livekit/rtp_transceiver.h"
#include "rust/cxx.h"
#include "rust_types.h"
#include "webrtc-sys/src/helper.rs.h"
#include "webrtc.h"

namespace livekit {
class NativeAddIceCandidateObserver;

class PeerConnection {
 public:
  explicit PeerConnection(
      std::shared_ptr<RTCRuntime> rtc_runtime,
      rtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection);

  void create_offer(NativeCreateSdpObserverHandle& observer,
                    RTCOfferAnswerOptions options) const;

  void create_answer(NativeCreateSdpObserverHandle& observer,
                     RTCOfferAnswerOptions options) const;

  void set_local_description(std::unique_ptr<SessionDescription> desc,
                             NativeSetLocalSdpObserverHandle& observer) const;

  void set_remote_description(std::unique_ptr<SessionDescription> desc,
                              NativeSetRemoteSdpObserverHandle& observer) const;

  std::unique_ptr<DataChannel> create_data_channel(
      rust::String label,
      std::unique_ptr<NativeDataChannelInit> init) const;

  void add_ice_candidate(std::shared_ptr<IceCandidate> candidate,
                         NativeAddIceCandidateObserver& observer) const;

  std::shared_ptr<RtpSender> add_track(
      std::shared_ptr<MediaStreamTrack> track,
      const rust::Vec<rust::String>& stream_ids) const;

  void remove_track(std::shared_ptr<RtpSender> sender) const;

  std::shared_ptr<RtpTransceiver> add_transceiver(
      std::shared_ptr<MediaStreamTrack> track,
      RtpTransceiverInit init) const;

  std::shared_ptr<RtpTransceiver> add_transceiver_for_media(
      MediaType media_type,
      RtpTransceiverInit init) const;

  rust::Vec<std::shared_ptr<RtpSender>> get_senders() const;

  rust::Vec<std::shared_ptr<RtpReceiver>> get_receivers() const;

  rust::Vec<std::shared_ptr<RtpTransceiver>> get_transceivers() const;

  std::unique_ptr<SessionDescription> local_description() const;

  std::unique_ptr<SessionDescription> remote_description() const;

  SignalingState signaling_state() const;

  IceGatheringState ice_gathering_state() const;

  IceConnectionState ice_connection_state() const;

  void close();

 private:
  std::shared_ptr<RTCRuntime> rtc_runtime_;
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
      std::shared_ptr<RTCRuntime> rtc_runtime,
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
  std::shared_ptr<RTCRuntime> rtc_runtime_;
  rust::Box<PeerConnectionObserverWrapper> observer_;
};

std::unique_ptr<NativePeerConnectionObserver>
create_native_peer_connection_observer(
    std::shared_ptr<RTCRuntime> rtc_runtime,
    rust::Box<PeerConnectionObserverWrapper> observer);
}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_PEER_CONNECTION_H
