/*
 * Copyright 2025 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#pragma once

#include <atomic>
#include <functional>
#include <memory>
#include <optional>

#include "api/peer_connection_interface.h"
#include "api/scoped_refptr.h"
#include "livekit/data_channel.h"
#include "livekit/helper.h"
#include "livekit/jsep.h"
#include "livekit/media_stream.h"
#include "livekit/rtc_error.h"
#include "livekit/rtp_receiver.h"
#include "livekit/rtp_sender.h"
#include "livekit/rtp_transceiver.h"
#include "livekit/webrtc.h"
#include "rust/cxx.h"
#include "webrtc-sys/src/data_channel.rs.h"

namespace livekit_ffi {
class PeerConnection;
}  // namespace livekit_ffi
#include "webrtc-sys/src/peer_connection.rs.h"

namespace livekit_ffi {

webrtc::PeerConnectionInterface::RTCConfiguration to_native_rtc_configuration(
    RtcConfiguration config);

class PeerConnectionObserverWrapper;

// Owns the Rust completion state of an asynchronous `AddIceCandidate` call.
//
// `PeerConnectionInterface::AddIceCandidate` takes a copyable
// `std::function<void(RTCError)>` and invokes it from libwebrtc's operations
// chain, which may run long after `PeerConnection::add_ice_candidate` has
// returned. The state therefore cannot be captured by reference from that
// frame, and the move-only `rust::Box<PeerContext>` cannot be captured by value
// into a copyable `std::function`. Instances are held behind a `shared_ptr` so
// that every copy libwebrtc makes of the callback refers to the same state, and
// `Complete` hands the context back to Rust exactly once no matter how many
// copies are invoked.
class AddIceCandidateCompletion {
 public:
  AddIceCandidateCompletion(
      rust::Box<PeerContext> ctx,
      rust::Fn<void(rust::Box<PeerContext>, RtcError)> on_complete)
      : ctx_(std::move(ctx)), on_complete_(on_complete) {}

  AddIceCandidateCompletion(const AddIceCandidateCompletion&) = delete;
  AddIceCandidateCompletion& operator=(const AddIceCandidateCompletion&) =
      delete;

  // Reports `error` to Rust. Subsequent calls are ignored: the context can only
  // be moved back to Rust once.
  void Complete(const webrtc::RTCError& error) {
    if (completed_.exchange(true, std::memory_order_acq_rel))
      return;

    on_complete_(std::move(*ctx_), to_error(error));
    ctx_.reset();
  }

 private:
  std::optional<rust::Box<PeerContext>> ctx_;
  rust::Fn<void(rust::Box<PeerContext>, RtcError)> on_complete_;
  std::atomic<bool> completed_{false};
};

// Builds the callback handed to `PeerConnectionInterface::AddIceCandidate`.
// The returned function keeps `ctx` and `on_complete` alive until libwebrtc
// invokes it (or drops every copy of it, which drops the context and cancels
// the pending Rust future).
std::function<void(const webrtc::RTCError&)> make_add_ice_candidate_callback(
    rust::Box<PeerContext> ctx,
    rust::Fn<void(rust::Box<PeerContext>, RtcError)> on_complete);

class PeerConnection : webrtc::PeerConnectionObserver {
 public:
  PeerConnection(
      std::shared_ptr<RtcRuntime> rtc_runtime,
      webrtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> pc_factory,
      rust::Box<PeerConnectionObserverWrapper> observer);

  ~PeerConnection();

  bool Initialize(webrtc::PeerConnectionInterface::RTCConfiguration config);

  void set_configuration(RtcConfiguration config) const;

  void create_offer(
      RtcOfferAnswerOptions options,
      rust::Box<PeerContext> ctx,
      rust::Fn<void(rust::Box<PeerContext>,
                    std::unique_ptr<SessionDescription>)> on_success,
      rust::Fn<void(rust::Box<PeerContext>, RtcError)> on_error) const;

  void create_answer(
      RtcOfferAnswerOptions options,
      rust::Box<PeerContext> ctx,
      rust::Fn<void(rust::Box<PeerContext>,
                    std::unique_ptr<SessionDescription>)> on_success,
      rust::Fn<void(rust::Box<PeerContext>, RtcError)> on_error) const;

  void set_local_description(
      std::unique_ptr<SessionDescription> desc,
      rust::Box<PeerContext> ctx,
      rust::Fn<void(rust::Box<PeerContext>, RtcError)> on_complete) const;

  void set_remote_description(
      std::unique_ptr<SessionDescription> desc,
      rust::Box<PeerContext> ctx,
      rust::Fn<void(rust::Box<PeerContext>, RtcError)> on_complete) const;

  std::shared_ptr<DataChannel> create_data_channel(rust::String label,
                                                   DataChannelInit init) const;

  void add_ice_candidate(
      std::shared_ptr<IceCandidate> candidate,
      rust::Box<PeerContext> ctx,
      rust::Fn<void(rust::Box<PeerContext>, RtcError)> on_complete) const;

  std::shared_ptr<RtpSender> add_track(
      std::shared_ptr<MediaStreamTrack> track,
      const rust::Vec<rust::String>& stream_ids) const;

  void remove_track(std::shared_ptr<RtpSender> sender) const;

  void get_stats(
      rust::Box<PeerContext> ctx,
      rust::Fn<void(rust::Box<PeerContext>, rust::String)> on_stats) const;

  void restart_ice() const;

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

  std::unique_ptr<SessionDescription> pending_local_description() const;

  std::unique_ptr<SessionDescription> pending_remote_description() const;

  std::unique_ptr<SessionDescription> local_description() const;

  std::unique_ptr<SessionDescription> remote_description() const;

  PeerConnectionState connection_state() const;

  SignalingState signaling_state() const;

  IceGatheringState ice_gathering_state() const;

  IceConnectionState ice_connection_state() const;

  void close() const;

  void OnSignalingChange(
      webrtc::PeerConnectionInterface::SignalingState new_state) override;

  void OnAddStream(
      webrtc::scoped_refptr<webrtc::MediaStreamInterface> stream) override;

  void OnRemoveStream(
      webrtc::scoped_refptr<webrtc::MediaStreamInterface> stream) override;

  void OnDataChannel(
      webrtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) override;

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

  void OnIceCandidate(const webrtc::IceCandidate* candidate) override;

  void OnIceCandidateError(const std::string& address,
                           int port,
                           const std::string& url,
                           int error_code,
                           const std::string& error_text) override;

  void OnIceCandidateRemoved(const webrtc::IceCandidate* candidate) override;

  void OnIceConnectionReceivingChange(bool receiving) override;

  void OnIceSelectedCandidatePairChanged(
      const webrtc::CandidatePairChangeEvent& event) override;

  void OnAddTrack(
      webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver,
      const std::vector<webrtc::scoped_refptr<webrtc::MediaStreamInterface>>&
          streams) override;

  void OnTrack(
      webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) override;

  void OnRemoveTrack(
      webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver) override;

  void OnInterestingUsage(int usage_pattern) override;

 private:
  std::shared_ptr<RtcRuntime> rtc_runtime_;
  webrtc::scoped_refptr<webrtc::PeerConnectionFactoryInterface> pc_factory_;
  rust::Box<PeerConnectionObserverWrapper> observer_;
  webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection_;
};

static std::shared_ptr<PeerConnection> _shared_peer_connection() {
  return nullptr;  // Ignore
}

#ifdef LIVEKIT_TEST
// Test-only seam reproducing how libwebrtc drives an `AddIceCandidate`
// completion: the callback outlives the frame that built it, is copied into the
// operations chain, and is then invoked `invocations` times (zero models a
// pending operation that is abandoned). Reports back to Rust at most once; an
// empty `error_message` reports success.
void complete_add_ice_candidate_for_test(
    rust::Box<PeerContext> ctx,
    rust::Fn<void(rust::Box<PeerContext>, RtcError)> on_complete,
    rust::String error_message,
    size_t invocations);
#endif

}  // namespace livekit_ffi
