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

#include "livekit/rtp_sender.h"
#include "livekit/jsep.h"

#include <memory>
#include <optional>

#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder_factory.h"
#include "rust/cxx.h"
#include "rtc_base/logging.h"
#include "webrtc-sys/src/rtp_sender.rs.h"

namespace livekit_ffi {

namespace {

constexpr char kBackendParameter[] = "x-livekit-video-encoder-backend";

const char* BackendName(VideoEncoderBackend backend) {
  switch (backend) {
    case VideoEncoderBackend::Auto:
      return "auto";
    case VideoEncoderBackend::Software:
      return "software";
    case VideoEncoderBackend::Hardware:
      return "hardware";
    case VideoEncoderBackend::Nvenc:
      return "nvenc";
    case VideoEncoderBackend::Vaapi:
      return "vaapi";
    case VideoEncoderBackend::VideoToolbox:
      return "videotoolbox";
    case VideoEncoderBackend::PreEncoded:
      return "preencoded";
  }
}

std::optional<VideoEncoderBackend> BackendFromFormat(
    const webrtc::SdpVideoFormat& format) {
  auto it = format.parameters.find(kBackendParameter);
  if (it == format.parameters.end()) {
    return std::nullopt;
  }

  if (it->second == BackendName(VideoEncoderBackend::Software)) {
    return VideoEncoderBackend::Software;
  }
  if (it->second == BackendName(VideoEncoderBackend::Hardware)) {
    return VideoEncoderBackend::Hardware;
  }
  if (it->second == BackendName(VideoEncoderBackend::Nvenc)) {
    return VideoEncoderBackend::Nvenc;
  }
  if (it->second == BackendName(VideoEncoderBackend::Vaapi)) {
    return VideoEncoderBackend::Vaapi;
  }
  if (it->second == BackendName(VideoEncoderBackend::VideoToolbox)) {
    return VideoEncoderBackend::VideoToolbox;
  }
  if (it->second == BackendName(VideoEncoderBackend::PreEncoded)) {
    return VideoEncoderBackend::PreEncoded;
  }

  return std::nullopt;
}

webrtc::SdpVideoFormat WithBackend(
    const webrtc::SdpVideoFormat& format,
    VideoEncoderBackend backend) {
  webrtc::SdpVideoFormat tagged = format;
  tagged.parameters[kBackendParameter] = BackendName(backend);
  return tagged;
}

class FixedVideoEncoderSelector final
    : public webrtc::VideoEncoderFactory::EncoderSelectorInterface {
 public:
  explicit FixedVideoEncoderSelector(VideoEncoderBackend backend)
      : backend_(backend) {}

  void OnCurrentEncoder(const webrtc::SdpVideoFormat& format) override {
    current_encoder_ = format;
    requested_ = BackendFromFormat(format) == backend_;
  }

  std::optional<webrtc::SdpVideoFormat> OnAvailableBitrate(
      const webrtc::DataRate& /* rate */) override {
    return SelectEncoder();
  }

  std::optional<webrtc::SdpVideoFormat> OnResolutionChange(
      const webrtc::RenderResolution& /* resolution */) override {
    return SelectEncoder();
  }

  std::optional<webrtc::SdpVideoFormat> OnEncoderBroken() override {
    // The preferred backend is a hard requirement for this sender (e.g.
    // pre-encoded pass-through). When the active encoder breaks — including
    // when the initial untagged encoder could not even be created — request
    // the preferred backend explicitly instead of giving up, so the sender
    // recovers onto the right encoder.
    if (!current_encoder_) {
      return std::nullopt;
    }
    requested_ = true;
    return WithBackend(*current_encoder_, backend_);
  }

 private:
  std::optional<webrtc::SdpVideoFormat> SelectEncoder() {
    if (requested_ || !current_encoder_) {
      return std::nullopt;
    }

    requested_ = true;
    return WithBackend(*current_encoder_, backend_);
  }

  VideoEncoderBackend backend_;
  bool requested_ = false;
  std::optional<webrtc::SdpVideoFormat> current_encoder_;
};

}  // namespace


RtpSender::RtpSender(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender,
    webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection)
    : rtc_runtime_(rtc_runtime),
      sender_(std::move(sender)),
      peer_connection_(std::move(peer_connection)) {}

bool RtpSender::set_track(std::shared_ptr<MediaStreamTrack> track) const {
  return sender_->SetTrack(track->rtc_track().get());
}

std::shared_ptr<MediaStreamTrack> RtpSender::track() const {
  return rtc_runtime_->get_or_create_media_stream_track(sender_->track());
}

uint32_t RtpSender::ssrc() const {
  return sender_->ssrc();
}

void RtpSender::get_stats(
    rust::Box<SenderContext> ctx,
    rust::Fn<void(rust::Box<SenderContext>, rust::String)> on_stats) const {
  auto observer =
      webrtc::make_ref_counted<NativeRtcStatsCollector<SenderContext>>(std::move(ctx), on_stats);
  peer_connection_->GetStats(sender_, observer);
}

MediaType RtpSender::media_type() const {
  return static_cast<MediaType>(sender_->media_type());
}

rust::String RtpSender::id() const {
  return sender_->id();
}

rust::Vec<rust::String> RtpSender::stream_ids() const {
  rust::Vec<rust::String> vec;
  for (auto str : sender_->stream_ids())
    vec.push_back(str);

  return vec;
}

void RtpSender::set_streams(const rust::Vec<rust::String>& stream_ids) const {
  std::vector<std::string> std_stream_ids(stream_ids.begin(), stream_ids.end());
  sender_->SetStreams(std_stream_ids);
}

rust::Vec<RtpEncodingParameters> RtpSender::init_send_encodings() const {
  rust::Vec<RtpEncodingParameters> encodings;
  for (auto encoding : sender_->init_send_encodings())
    encodings.push_back(to_rust_rtp_encoding_parameters(encoding));
  return encodings;
}

RtpParameters RtpSender::get_parameters() const {
  return to_rust_rtp_parameters(sender_->GetParameters());
}

void RtpSender::set_parameters(RtpParameters params) const {
  auto error = sender_->SetParameters(to_native_rtp_parameters(params));
  if (!error.ok())
    throw std::runtime_error(serialize_error(to_error(error)));
}

void RtpSender::set_video_encoder_backend(VideoEncoderBackend backend) const {
  if (sender_->media_type() != webrtc::MediaType::VIDEO) {
    RTC_LOG(LS_WARNING)
        << "Ignoring video encoder backend preference on non-video sender.";
    return;
  }

  if (backend == VideoEncoderBackend::Auto) {
    sender_->SetEncoderSelector(
        std::unique_ptr<webrtc::VideoEncoderFactory::EncoderSelectorInterface>());
    return;
  }

  sender_->SetEncoderSelector(
      std::make_unique<FixedVideoEncoderSelector>(backend));
}

}  // namespace livekit_ffi
