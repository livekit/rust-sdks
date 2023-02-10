//
// Created by Théo Monnom on 02/09/2022.
//

#include "livekit/rtp_transceiver.h"

#include "webrtc-sys/src/rtp_transceiver.rs.h"

namespace livekit {
RtpTransceiver::RtpTransceiver(
    rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver)
    : transceiver_(std::move(transceiver)) {}

MediaType RtpTransceiver::media_type() const {
  return static_cast<MediaType>(transceiver_->media_type());
}

rust::String RtpTransceiver::mid() const {
  // The error/Result is converted into an Option in Rust (Wait for Option
  // suport in cxx.rs) (value throws an error if there's no value)
  return transceiver_->mid().value();
}

std::shared_ptr<RtpSender> RtpTransceiver::sender() const {
  return std::make_shared<RtpSender>(transceiver_->sender());
}

std::shared_ptr<RtpReceiver> RtpTransceiver::receiver() const {
  return std::make_shared<RtpReceiver>(transceiver_->receiver());
}

bool RtpTransceiver::stopped() const {
  return transceiver_->stopped();
}

bool RtpTransceiver::stopping() const {
  return transceiver_->stopping();
}

RtpTransceiverDirection RtpTransceiver::direction() const {
  return static_cast<RtpTransceiverDirection>(transceiver_->direction());
}

void RtpTransceiver::set_direction(RtpTransceiverDirection direction) const {
  auto error = transceiver_->SetDirectionWithError(
      static_cast<webrtc::RtpTransceiverDirection>(direction));

  if (!error.ok()) {
    throw std::runtime_error(serialize_error(to_error(error)));
  }
}

RtpTransceiverDirection RtpTransceiver::current_direction() const {
  return static_cast<RtpTransceiverDirection>(
      transceiver_->current_direction().value());
}

RtpTransceiverDirection RtpTransceiver::fired_direction() const {
  return static_cast<RtpTransceiverDirection>(
      transceiver_->fired_direction().value());
}

void RtpTransceiver::stop_standard() const {
  auto error = transceiver_->StopStandard();
  if (!error.ok())
    throw std::runtime_error(serialize_error(to_error(error)));
}

void RtpTransceiver::set_codec_preferences(
    rust::Vec<RtpCodecCapability> codecs) const {
  std::vector<webrtc::RtpCodecCapability> std_codecs;
  for (auto codec : codecs)
    std_codecs.push_back(to_native_rtp_codec_capability(codec));

  auto error = transceiver_->SetCodecPreferences(std_codecs);
  if (!error.ok())
    throw std::runtime_error(serialize_error(to_error(error)));
}

rust::Vec<RtpCodecCapability> RtpTransceiver::codec_preferences() const {
  rust::Vec<RtpCodecCapability> rust;
  for (auto codec : transceiver_->codec_preferences())
    rust.push_back(to_rust_rtp_codec_capability(codec));

  return rust;
}

rust::Vec<RtpHeaderExtensionCapability>
RtpTransceiver::header_extensions_to_offer() const {
  rust::Vec<RtpHeaderExtensionCapability> rust;
  for (auto header : transceiver_->HeaderExtensionsToOffer())
    rust.push_back(to_rust_rtp_header_extension_capability(header));

  return rust;
}

rust::Vec<RtpHeaderExtensionCapability>
RtpTransceiver::header_extensions_negotiated() const {
  rust::Vec<RtpHeaderExtensionCapability> rust;
  for (auto header : transceiver_->HeaderExtensionsNegotiated())
    rust.push_back(to_rust_rtp_header_extension_capability(header));

  return rust;
}

void RtpTransceiver::set_offered_rtp_header_extensions(
    rust::Vec<RtpHeaderExtensionCapability> header_extensions_to_offer) const {
  std::vector<webrtc::RtpHeaderExtensionCapability> headers;

  for (auto header : header_extensions_to_offer)
    headers.push_back(to_native_rtp_header_extension_capability(header));

  auto error = transceiver_->SetOfferedRtpHeaderExtensions(headers);
  if (!error.ok())
    throw std::runtime_error(serialize_error(to_error(error)));
}

}  // namespace livekit
