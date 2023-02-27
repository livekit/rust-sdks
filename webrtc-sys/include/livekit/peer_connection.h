/*
 * Copyright 2023 LiveKit
 *
 * Licensed under the Apache License, Version 2.0 (the “License”);
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an “AS IS” BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#pragma once

#include <memory>

#include "api/peer_connection_interface.h"
#include "livekit/data_channel.h"
#include "livekit/helper.h"
#include "livekit/jsep.h"
#include "livekit/media_stream.h"
#include "livekit/rtp_receiver.h"
#include "livekit/rtp_sender.h"
#include "livekit/rtp_transceiver.h"
#include "livekit/webrtc.h"
#include "rust/cxx.h"

namespace livekit {
class NativeAddIceCandidateObserver;
class PeerConnection;
class NativeAddIceCandidateObserver;
class NativePeerConnectionObserver;
}  // namespace livekit
#include "webrtc-sys/src/peer_connection.rs.h"

namespace livekit {

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

  std::shared_ptr<DataChannel> create_data_channel(
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

  rust::Vec<RtpSenderPtr> get_senders() const;

  rust::Vec<RtpReceiverPtr> get_receivers() const;

  rust::Vec<RtpTransceiverPtr> get_transceivers() const;

  std::unique_ptr<SessionDescription> current_local_description() const;

  std::unique_ptr<SessionDescription> current_remote_description() const;

  PeerConnectionState connection_state() const;

  SignalingState signaling_state() const;

  IceGatheringState ice_gathering_state() const;

  IceConnectionState ice_connection_state() const;

  void close() const;

 private:
  std::shared_ptr<RTCRuntime> rtc_runtime_;
  rtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection_;
};

static std::shared_ptr<PeerConnection> _shared_peer_connection() {
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

std::shared_ptr<NativePeerConnectionObserver>
create_native_peer_connection_observer(
    std::shared_ptr<RTCRuntime> rtc_runtime,
    rust::Box<PeerConnectionObserverWrapper> observer);
}  // namespace livekit
