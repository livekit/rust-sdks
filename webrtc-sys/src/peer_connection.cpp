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

#include "livekit/peer_connection.h"

#include <memory>

#include "api/data_channel_interface.h"
#include "api/scoped_refptr.h"
#include "livekit/data_channel.h"
#include "livekit/jsep.h"
#include "livekit/media_stream.h"
#include "livekit/rtc_error.h"
#include "livekit/rtp_transceiver.h"
#include "rtc_base/logging.h"
#include "webrtc-sys/src/peer_connection.rs.h"
#include "webrtc-sys/src/rtc_error.rs.h"

namespace livekit {

inline webrtc::PeerConnectionInterface::RTCOfferAnswerOptions
to_native_offer_answer_options(const RtcOfferAnswerOptions& options) {
  webrtc::PeerConnectionInterface::RTCOfferAnswerOptions rtc_options;
  rtc_options.offer_to_receive_video = options.offer_to_receive_video;
  rtc_options.offer_to_receive_audio = options.offer_to_receive_audio;
  rtc_options.voice_activity_detection = options.voice_activity_detection;
  rtc_options.ice_restart = options.ice_restart;
  rtc_options.use_rtp_mux = options.use_rtp_mux;
  rtc_options.raw_packetization_for_video = options.raw_packetization_for_video;
  rtc_options.num_simulcast_layers = options.num_simulcast_layers;
  rtc_options.use_obsolete_sctp_sdp = options.use_obsolete_sctp_sdp;
  return rtc_options;
}

PeerConnection::PeerConnection(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    std::unique_ptr<NativePeerConnectionObserver> observer,
    rtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection)
    : rtc_runtime_(rtc_runtime),
      observer_(std::move(observer)),
      peer_connection_(std::move(peer_connection)) {
  RTC_LOG(LS_VERBOSE) << "PeerConnection::PeerConnection()";
}

PeerConnection::~PeerConnection() {
  RTC_LOG(LS_VERBOSE) << "PeerConnection::~PeerConnection()";
}

void PeerConnection::create_offer(
    RtcOfferAnswerOptions options,
    rust::Box<AsyncContext> ctx,
    rust::Fn<void(rust::Box<AsyncContext>, std::unique_ptr<SessionDescription>)>
        on_success,
    rust::Fn<void(rust::Box<AsyncContext>, RtcError)> on_error) const {
  rtc::scoped_refptr<NativeCreateSdpObserver> observer =
      rtc::make_ref_counted<NativeCreateSdpObserver>(std::move(ctx), on_success,
                                                     on_error);

  peer_connection_->CreateOffer(observer.get(),
                                to_native_offer_answer_options(options));
}

void PeerConnection::create_answer(
    RtcOfferAnswerOptions options,
    rust::Box<AsyncContext> ctx,
    rust::Fn<void(rust::Box<AsyncContext>, std::unique_ptr<SessionDescription>)>
        on_success,
    rust::Fn<void(rust::Box<AsyncContext>, RtcError)> on_error) const {
  rtc::scoped_refptr<NativeCreateSdpObserver> observer =
      rtc::make_ref_counted<NativeCreateSdpObserver>(std::move(ctx), on_success,
                                                     on_error);

  peer_connection_->CreateAnswer(observer.get(),
                                 to_native_offer_answer_options(options));
}

void PeerConnection::set_local_description(
    std::unique_ptr<SessionDescription> desc,
    rust::Box<AsyncContext> ctx,
    rust::Fn<void(rust::Box<AsyncContext>, RtcError)> on_complete) const {
  rtc::scoped_refptr<NativeSetLocalSdpObserver> observer =
      rtc::make_ref_counted<NativeSetLocalSdpObserver>(std::move(ctx),
                                                       on_complete);

  peer_connection_->SetLocalDescription(desc->clone()->release(), observer);
}

void PeerConnection::set_remote_description(
    std::unique_ptr<SessionDescription> desc,
    rust::Box<AsyncContext> ctx,
    rust::Fn<void(rust::Box<AsyncContext>, RtcError)> on_complete) const {
  rtc::scoped_refptr<NativeSetRemoteSdpObserver> observer =
      rtc::make_ref_counted<NativeSetRemoteSdpObserver>(std::move(ctx),
                                                        on_complete);

  peer_connection_->SetRemoteDescription(desc->clone()->release(), observer);
}

void PeerConnection::add_ice_candidate(
    std::shared_ptr<IceCandidate> candidate,
    rust::Box<AsyncContext> ctx,
    rust::Fn<void(rust::Box<AsyncContext>, RtcError)> on_complete) const {
  peer_connection_->AddIceCandidate(
      candidate->release(), [&](const webrtc::RTCError& err) {
        on_complete(std::move(ctx), to_error(err));
      });
}

std::shared_ptr<DataChannel> PeerConnection::create_data_channel(
    rust::String label,
    DataChannelInit init) const {
  webrtc::DataChannelInit rtc_init = to_native_data_channel_init(init);
  auto result =
      peer_connection_->CreateDataChannelOrError(label.c_str(), &rtc_init);

  if (!result.ok()) {
    throw std::runtime_error(serialize_error(to_error(result.error())));
  }

  return std::make_shared<DataChannel>(rtc_runtime_, result.value());
}

std::shared_ptr<RtpSender> PeerConnection::add_track(
    std::shared_ptr<MediaStreamTrack> track,
    const rust::Vec<rust::String>& stream_ids) const {
  std::vector<std::string> std_stream_ids(stream_ids.begin(), stream_ids.end());
  auto result = peer_connection_->AddTrack(track->rtc_track(), std_stream_ids);
  if (!result.ok()) {
    throw std::runtime_error(serialize_error(to_error(result.error())));
  }

  return std::make_shared<RtpSender>(rtc_runtime_, result.value());
}

void PeerConnection::remove_track(std::shared_ptr<RtpSender> sender) const {
  auto error = peer_connection_->RemoveTrackOrError(sender->rtc_sender());
  if (!error.ok())
    throw std::runtime_error(serialize_error(to_error(error)));
}

std::shared_ptr<RtpTransceiver> PeerConnection::add_transceiver(
    std::shared_ptr<MediaStreamTrack> track,
    RtpTransceiverInit init) const {
  auto result = peer_connection_->AddTransceiver(
      track->rtc_track(), to_native_rtp_transceiver_init(init));
  if (!result.ok())
    throw std::runtime_error(serialize_error(to_error(result.error())));

  return std::make_shared<RtpTransceiver>(rtc_runtime_, result.value());
}

std::shared_ptr<RtpTransceiver> PeerConnection::add_transceiver_for_media(
    MediaType media_type,
    RtpTransceiverInit init) const {
  auto result = peer_connection_->AddTransceiver(
      static_cast<cricket::MediaType>(media_type),
      to_native_rtp_transceiver_init(init));

  if (!result.ok())
    throw std::runtime_error(serialize_error(to_error(result.error())));

  return std::make_shared<RtpTransceiver>(rtc_runtime_, result.value());
}

rust::Vec<RtpSenderPtr> PeerConnection::get_senders() const {
  rust::Vec<RtpSenderPtr> vec;
  for (auto sender : peer_connection_->GetSenders())
    vec.push_back(
        RtpSenderPtr{std::make_shared<RtpSender>(rtc_runtime_, sender)});

  return vec;
}

rust::Vec<RtpReceiverPtr> PeerConnection::get_receivers() const {
  rust::Vec<RtpReceiverPtr> vec;
  for (auto receiver : peer_connection_->GetReceivers())
    vec.push_back(
        RtpReceiverPtr{std::make_shared<RtpReceiver>(rtc_runtime_, receiver)});

  return vec;
}

rust::Vec<RtpTransceiverPtr> PeerConnection::get_transceivers() const {
  rust::Vec<RtpTransceiverPtr> vec;
  for (auto transceiver : peer_connection_->GetTransceivers())
    vec.push_back(RtpTransceiverPtr{
        std::make_shared<RtpTransceiver>(rtc_runtime_, transceiver)});

  return vec;
}

std::unique_ptr<SessionDescription> PeerConnection::current_local_description()
    const {
  auto local_description = peer_connection_->current_local_description();
  if (local_description)
    return std::make_unique<SessionDescription>(local_description->Clone());

  return nullptr;
}

std::unique_ptr<SessionDescription> PeerConnection::current_remote_description()
    const {
  auto remote_description = peer_connection_->current_remote_description();
  if (remote_description)
    return std::make_unique<SessionDescription>(remote_description->Clone());

  return nullptr;
}

std::unique_ptr<SessionDescription> PeerConnection::pending_local_description()
    const {
  auto local_description = peer_connection_->pending_local_description();
  if (local_description)
    return std::make_unique<SessionDescription>(local_description->Clone());

  return nullptr;
}

std::unique_ptr<SessionDescription> PeerConnection::pending_remote_description()
    const {
  auto remote_description = peer_connection_->pending_remote_description();
  if (remote_description)
    return std::make_unique<SessionDescription>(remote_description->Clone());

  return nullptr;
}

std::unique_ptr<SessionDescription> PeerConnection::local_description() const {
  auto local_description = peer_connection_->local_description();
  if (local_description)
    return std::make_unique<SessionDescription>(local_description->Clone());

  return nullptr;
}

std::unique_ptr<SessionDescription> PeerConnection::remote_description() const {
  auto remote_description = peer_connection_->remote_description();
  if (remote_description)
    return std::make_unique<SessionDescription>(remote_description->Clone());

  return nullptr;
}

PeerConnectionState PeerConnection::connection_state() const {
  return static_cast<PeerConnectionState>(
      peer_connection_->peer_connection_state());
}

SignalingState PeerConnection::signaling_state() const {
  return static_cast<SignalingState>(peer_connection_->signaling_state());
}

IceGatheringState PeerConnection::ice_gathering_state() const {
  return static_cast<IceGatheringState>(
      peer_connection_->ice_gathering_state());
}

IceConnectionState PeerConnection::ice_connection_state() const {
  return static_cast<IceConnectionState>(
      peer_connection_->ice_connection_state());
}

void PeerConnection::close() const {
  peer_connection_->Close();
}

// PeerConnectionObserver

NativePeerConnectionObserver::NativePeerConnectionObserver(
    rust::Box<PeerConnectionObserverWrapper> observer)
    : observer_(std::move(observer)) {
  RTC_LOG(LS_INFO) << "NativePeerConnectionObserver()";
}

NativePeerConnectionObserver::~NativePeerConnectionObserver() {
  RTC_LOG(LS_INFO) << "~NativePeerConnectionObserver()";
}

void NativePeerConnectionObserver::OnSignalingChange(
    webrtc::PeerConnectionInterface::SignalingState new_state) {
  observer_->on_signaling_change(static_cast<SignalingState>(new_state));
}

void NativePeerConnectionObserver::OnAddStream(
    rtc::scoped_refptr<webrtc::MediaStreamInterface> stream) {
  observer_->on_add_stream(std::make_unique<MediaStream>(rtc_runtime_, stream));
}

void NativePeerConnectionObserver::OnRemoveStream(
    rtc::scoped_refptr<webrtc::MediaStreamInterface> stream) {
  // Find current MediaStream
  // observer_->on_remove_stream(std::make_unique<MediaStream>(rtc_runtime_,
  // stream));
}

void NativePeerConnectionObserver::OnDataChannel(
    rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) {
  observer_->on_data_channel(
      std::make_shared<DataChannel>(rtc_runtime_, data_channel));
}

void NativePeerConnectionObserver::OnRenegotiationNeeded() {
  observer_->on_renegotiation_needed();
}

void NativePeerConnectionObserver::OnNegotiationNeededEvent(uint32_t event_id) {
  observer_->on_negotiation_needed_event(event_id);
}

void NativePeerConnectionObserver::OnIceConnectionChange(
    webrtc::PeerConnectionInterface::IceConnectionState new_state) {
  observer_->on_ice_connection_change(
      static_cast<IceConnectionState>(new_state));
}

void NativePeerConnectionObserver::OnStandardizedIceConnectionChange(
    webrtc::PeerConnectionInterface::IceConnectionState new_state) {
  observer_->on_standardized_ice_connection_change(
      static_cast<IceConnectionState>(new_state));
}

void NativePeerConnectionObserver::OnConnectionChange(
    webrtc::PeerConnectionInterface::PeerConnectionState new_state) {
  observer_->on_connection_change(static_cast<PeerConnectionState>(new_state));
}

void NativePeerConnectionObserver::OnIceGatheringChange(
    webrtc::PeerConnectionInterface::IceGatheringState new_state) {
  observer_->on_ice_gathering_change(static_cast<IceGatheringState>(new_state));
}

void NativePeerConnectionObserver::OnIceCandidate(
    const webrtc::IceCandidateInterface* candidate) {
  auto new_candidate = webrtc::CreateIceCandidate(candidate->sdp_mid(),
                                                  candidate->sdp_mline_index(),
                                                  candidate->candidate());
  observer_->on_ice_candidate(
      std::make_unique<IceCandidate>(std::move(new_candidate)));
}

void NativePeerConnectionObserver::OnIceCandidateError(
    const std::string& address,
    int port,
    const std::string& url,
    int error_code,
    const std::string& error_text) {
  observer_->on_ice_candidate_error(address, port, url, error_code, error_text);
}

void NativePeerConnectionObserver::OnIceCandidatesRemoved(
    const std::vector<cricket::Candidate>& candidates) {
  rust::Vec<CandidatePtr> vec;

  for (const auto& item : candidates) {
    vec.push_back(CandidatePtr{std::make_unique<Candidate>(item)});
  }

  observer_->on_ice_candidates_removed(std::move(vec));
}

void NativePeerConnectionObserver::OnIceConnectionReceivingChange(
    bool receiving) {
  observer_->on_ice_connection_receiving_change(receiving);
}

void NativePeerConnectionObserver::OnIceSelectedCandidatePairChanged(
    const cricket::CandidatePairChangeEvent& event) {
  CandidatePairChangeEvent e{};
  e.selected_candidate_pair.local =
      std::make_unique<Candidate>(event.selected_candidate_pair.local);
  e.selected_candidate_pair.remote =
      std::make_unique<Candidate>(event.selected_candidate_pair.remote);
  e.last_data_received_ms = event.last_data_received_ms;
  e.reason = event.reason;
  e.estimated_disconnected_time_ms = event.estimated_disconnected_time_ms;

  observer_->on_ice_selected_candidate_pair_changed(std::move(e));
}

void NativePeerConnectionObserver::OnAddTrack(
    rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver,
    const std::vector<rtc::scoped_refptr<webrtc::MediaStreamInterface>>&
        streams) {
  rust::Vec<MediaStreamPtr> vec;

  for (const auto& item : streams) {
    vec.push_back(
        MediaStreamPtr{std::make_unique<MediaStream>(rtc_runtime_, item)});
  }

  observer_->on_add_track(std::make_unique<RtpReceiver>(rtc_runtime_, receiver),
                          std::move(vec));
}

void NativePeerConnectionObserver::OnTrack(
    rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) {
  observer_->on_track(
      std::make_unique<RtpTransceiver>(rtc_runtime_, transceiver));
}

void NativePeerConnectionObserver::OnRemoveTrack(
    rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver) {
  observer_->on_remove_track(
      std::make_unique<RtpReceiver>(rtc_runtime_, receiver));
}

void NativePeerConnectionObserver::OnInterestingUsage(int usage_pattern) {
  observer_->on_interesting_usage(usage_pattern);
}

std::unique_ptr<NativePeerConnectionObserver>
create_native_peer_connection_observer(
    rust::Box<PeerConnectionObserverWrapper> observer) {
  return std::make_unique<NativePeerConnectionObserver>(std::move(observer));
}

}  // namespace livekit
