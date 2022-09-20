//
// Created by Théo Monnom on 30/08/2022.
//

#include "livekit/peer_connection.h"

#include "libwebrtc-sys/src/peer_connection.rs.h"
#include "livekit/rtc_error.h"

namespace livekit {

inline webrtc::PeerConnectionInterface::RTCOfferAnswerOptions
toNativeOfferAnswerOptions(const RTCOfferAnswerOptions& options) {
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
    rtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection)
    : peer_connection_(std::move(peer_connection)) {}

void PeerConnection::create_offer(
    NativeCreateSdpObserverHandle& observer_handle,
    RTCOfferAnswerOptions options) {
  peer_connection_->CreateOffer(observer_handle.observer.get(),
                                toNativeOfferAnswerOptions(options));
}

void PeerConnection::create_answer(
    NativeCreateSdpObserverHandle& observer_handle,
    RTCOfferAnswerOptions options) {
  peer_connection_->CreateAnswer(observer_handle.observer.get(),
                                 toNativeOfferAnswerOptions(options));
}

void PeerConnection::set_local_description(
    std::unique_ptr<SessionDescription> desc,
    NativeSetLocalSdpObserverHandle& observer) {
  peer_connection_->SetLocalDescription(desc->clone()->release(),
                                        observer.observer);
}

void PeerConnection::set_remote_description(
    std::unique_ptr<SessionDescription> desc,
    NativeSetRemoteSdpObserverHandle& observer) {
  peer_connection_->SetRemoteDescription(desc->clone()->release(),
                                         observer.observer);
}

std::unique_ptr<DataChannel> PeerConnection::create_data_channel(
    rust::String label,
    std::unique_ptr<NativeDataChannelInit> init) {
  auto result =
      peer_connection_->CreateDataChannelOrError(label.c_str(), init.get());

  if (!result.ok()) {
    throw std::runtime_error(serialize_error(to_error(result.error())));
  }

  return std::make_unique<DataChannel>(result.value());
}

void PeerConnection::add_ice_candidate(
    std::unique_ptr<IceCandidate> candidate,
    NativeAddIceCandidateObserver& observer) {
  peer_connection_->AddIceCandidate(
      candidate->release(),
      [&](const webrtc::RTCError& err) { observer.OnComplete(to_error(err)); });
}

void PeerConnection::close() {
  peer_connection_->Close();
}

// AddIceCandidateObserver

NativeAddIceCandidateObserver::NativeAddIceCandidateObserver(
    rust::Box<AddIceCandidateObserverWrapper> observer)
    : observer_(std::move(observer)) {}

void NativeAddIceCandidateObserver::OnComplete(const RTCError& error) {
  observer_->on_complete(error);
}

std::unique_ptr<NativeAddIceCandidateObserver>
create_native_add_ice_candidate_observer(
    rust::Box<AddIceCandidateObserverWrapper> observer) {
  return std::make_unique<NativeAddIceCandidateObserver>(std::move(observer));
}

// PeerConnectionObserver

NativePeerConnectionObserver::NativePeerConnectionObserver(
    rust::Box<PeerConnectionObserverWrapper> observer)
    : observer_(std::move(observer)) {}

void NativePeerConnectionObserver::OnSignalingChange(
    webrtc::PeerConnectionInterface::SignalingState new_state) {
  observer_->on_signaling_change(static_cast<SignalingState>(new_state));
}

void NativePeerConnectionObserver::OnAddStream(
    rtc::scoped_refptr<webrtc::MediaStreamInterface> stream) {
  observer_->on_add_stream(std::make_unique<MediaStreamInterface>(stream));
}

void NativePeerConnectionObserver::OnRemoveStream(
    rtc::scoped_refptr<webrtc::MediaStreamInterface> stream) {
  observer_->on_remove_stream(std::make_unique<MediaStreamInterface>(stream));
}

void NativePeerConnectionObserver::OnDataChannel(
    rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) {
  observer_->on_data_channel(std::make_unique<DataChannel>(data_channel));
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
  CandidatePairChangeEvent e;
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
    vec.push_back(MediaStreamPtr{std::make_unique<MediaStreamInterface>(item)});
  }

  observer_->on_add_track(std::make_unique<RtpReceiver>(receiver),
                          std::move(vec));
}

void NativePeerConnectionObserver::OnTrack(
    rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver) {
  observer_->on_track(std::make_unique<RtpTransceiver>(transceiver));
}

void NativePeerConnectionObserver::OnRemoveTrack(
    rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver) {
  observer_->on_remove_track(std::make_unique<RtpReceiver>(receiver));
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