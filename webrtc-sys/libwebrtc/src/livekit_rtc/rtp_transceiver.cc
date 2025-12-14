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

#include "livekit_rtc/rtp_transceiver.h"

#include "api/peer_connection_interface.h"
#include "api/scoped_refptr.h"

namespace livekit {

webrtc::RtpTransceiverInit to_native_rtp_transceiver_init(
    RtpTransceiverInit init) {
  {
    webrtc::RtpTransceiverInit native{};
    native.direction =
        static_cast<webrtc::RtpTransceiverDirection>(init.direction);
    native.stream_ids = std::vector<std::string>(init.stream_ids.begin(),
                                                 init.stream_ids.end());
    for (auto encoding : init.send_encodings)
      native.send_encodings.push_back(
          to_native_rtp_encoding_paramters(encoding));
    return native;
  }
}

RtpTransceiver::RtpTransceiver(
    webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver,
    webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection)
    : transceiver_(std::move(transceiver)),
      peer_connection_(std::move(peer_connection)) {}

lkMediaType RtpTransceiver::media_type() const {
  return static_cast<lkMediaType>(transceiver_->media_type());
}

std::string RtpTransceiver::mid() const {
  // The error/Result is converted into an Option in Rust (Wait for Option
  // suport in cxx.rs) (value throws an error if there's no value)
  return transceiver_->mid().value();
}

webrtc::scoped_refptr<RtpSender> RtpTransceiver::sender() const {
  return webrtc::make_ref_counted<RtpSender>(transceiver_->sender(),
                                             peer_connection_);
}

webrtc::scoped_refptr<RtpReceiver> RtpTransceiver::receiver() const {
  return webrtc::make_ref_counted<RtpReceiver>(transceiver_->receiver(),
                                               peer_connection_);
}

bool RtpTransceiver::stopped() const {
  return transceiver_->stopped();
}

bool RtpTransceiver::stopping() const {
  return transceiver_->stopping();
}

lkRtpTransceiverDirection RtpTransceiver::direction() const {
  return static_cast<lkRtpTransceiverDirection>(transceiver_->direction());
}

void RtpTransceiver::set_direction(lkRtpTransceiverDirection direction) const {
  auto error = transceiver_->SetDirectionWithError(
      static_cast<webrtc::RtpTransceiverDirection>(direction));

  if (!error.ok()) {
    // throw std::runtime_error(serialize_error(to_error(error)));
  }
}

lkRtpTransceiverDirection RtpTransceiver::current_direction() const {
  return static_cast<lkRtpTransceiverDirection>(
      transceiver_->current_direction().value());
}

lkRtpTransceiverDirection RtpTransceiver::fired_direction() const {
  return static_cast<lkRtpTransceiverDirection>(
      transceiver_->fired_direction().value());
}

void RtpTransceiver::stop_standard() const {
  auto error = transceiver_->StopStandard();
  if (!error.ok()) {
    // throw std::runtime_error(serialize_error(to_error(error)));
  }
}

void RtpTransceiver::set_codec_preferences(
    std::vector<RtpCodecCapability> codecs) const {
  std::vector<webrtc::RtpCodecCapability> std_codecs;

  for (auto codec : codecs)
    std_codecs.push_back(to_native_rtp_codec_capability(codec));

  auto error = transceiver_->SetCodecPreferences(std_codecs);
  if (!error.ok()) {
    // throw std::runtime_error(serialize_error(to_error(error)));
  }
}

std::vector<RtpCodecCapability> RtpTransceiver::codec_preferences() const {
  std::vector<RtpCodecCapability> rust;
  for (auto codec : transceiver_->codec_preferences())
    rust.push_back(to_capi_rtp_codec_capability(codec));

  return rust;
}

std::vector<RtpHeaderExtensionCapability>
RtpTransceiver::header_extensions_to_negotiate() const {
  std::vector<RtpHeaderExtensionCapability> rust;
  for (auto header : transceiver_->GetHeaderExtensionsToNegotiate())
    rust.push_back(to_capi_rtp_header_extension_capability(header));

  return rust;
}

std::vector<RtpHeaderExtensionCapability>
RtpTransceiver::negotiated_header_extensions() const {
  std::vector<RtpHeaderExtensionCapability> rust;
  for (auto header : transceiver_->GetNegotiatedHeaderExtensions())
    rust.push_back(to_capi_rtp_header_extension_capability(header));

  return rust;
}

void RtpTransceiver::set_header_extensions_to_negotiate(
    std::vector<RtpHeaderExtensionCapability> header_extensions_to_offer)
    const {
  std::vector<webrtc::RtpHeaderExtensionCapability> headers;

  for (auto header : header_extensions_to_offer)
    headers.push_back(to_native_rtp_header_extension_capability(header));

  auto error = transceiver_->SetHeaderExtensionsToNegotiate(headers);
  if (!error.ok()) {
    // throw std::runtime_error(serialize_error(to_error(error)));
  }
}

}  // namespace livekit
