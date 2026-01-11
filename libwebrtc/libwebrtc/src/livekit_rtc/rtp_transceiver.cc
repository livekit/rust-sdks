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

RtpTransceiver::RtpTransceiver(
    webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver,
    webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection)
    : transceiver_(std::move(transceiver)),
      peer_connection_(std::move(peer_connection)) {}

lkMediaType RtpTransceiver::media_type() const {
  return static_cast<lkMediaType>(transceiver_->media_type());
}


std::string RtpTransceiver::mid() const {
  return transceiver_->mid().value_or("");
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
    // TODO: handle error
    // throw std::runtime_error(serialize_error(to_error(error)));
  }
}

bool RtpTransceiver::stop_with_error(lkRtcError* error) const {
  auto rtc_err = transceiver_->StopStandard();
  if (!rtc_err.ok()) {
    // TODO: handle error
    // *error = to_error(rtc_err);
    return false;
  }
  return true;
}

void RtpTransceiver::set_codec_preferences(
    std::vector<webrtc::scoped_refptr<RtpCodecCapability>> codecs) const {
  std::vector<webrtc::RtpCodecCapability> std_codecs;

  for (auto codec : codecs)
    std_codecs.push_back(codec->rtc_capability);

  auto error = transceiver_->SetCodecPreferences(std_codecs);
  if (!error.ok()) {
    //TODO: handle error
    // throw std::runtime_error(serialize_error(to_error(error)));
  }
}

bool RtpTransceiver::lk_set_codec_preferences(lkVectorGeneric* codecs,
                                              lkRtcError* err_out) const {
  std::vector<webrtc::RtpCodecCapability> std_codecs;

  auto vec =
      reinterpret_cast<LKVector<webrtc::scoped_refptr<RtpCodecCapability>>*>(
          codecs);
  for (size_t i = 0; i < vec->size(); i++) {
    std_codecs.push_back(vec->get_at(i)->rtc_capability);
  }

  auto error = transceiver_->SetCodecPreferences(std_codecs);
  if (!error.ok()) {
    //TODO: handle error
    // *err_out = to_error(error);
    return false;
  }
  return true;
}

std::vector<webrtc::scoped_refptr<RtpCodecCapability>>
RtpTransceiver::codec_preferences() const {
  std::vector<webrtc::scoped_refptr<RtpCodecCapability>> capi;
  for (auto codec : transceiver_->codec_preferences()) {
    capi.push_back(RtpCodecCapability::FromNative(codec));
  }

  return capi;
}

}  // namespace livekit
