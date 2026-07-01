/*
 * Copyright 2026 LiveKit, Inc.
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
 
 #include "av1_encoder_impl.h"

#include <algorithm>
#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>

#include "api/video/video_codec_constants.h"
#include "api/video_codecs/scalability_mode.h"
#include "common_video/libyuv/include/webrtc_libyuv.h"
#include "jetson_av1_bitstream.h"
#include "livekit/dmabuf_video_frame_buffer.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "modules/video_coding/utility/simulcast_rate_allocator.h"
#include "rtc_base/checks.h"
#include "rtc_base/logging.h"
#include "system_wrappers/include/metrics.h"

namespace webrtc {

// Used by histograms. Values of entries should not be changed.
enum AV1EncoderImplEvent {
  kAV1EncoderEventInit = 0,
  kAV1EncoderEventError = 1,
  kAV1EncoderEventMax = 16,
};

namespace {

void DumpAv1PacketIfRequested(const std::vector<uint8_t>& packet,
                              bool keyframe) {
  static std::atomic<bool> dumped(false);
  if (dumped.load(std::memory_order_relaxed)) {
    return;
  }

  const char* dump_path = std::getenv("LK_DUMP_AV1");
  if (!dump_path || dump_path[0] == '\0') {
    return;
  }

  std::error_code ec;
  std::filesystem::path path(dump_path);
  if (path.has_parent_path()) {
    std::filesystem::create_directories(path.parent_path(), ec);
  }
  std::ofstream out(dump_path, std::ios::binary);
  if (!out.good()) {
    std::fprintf(stderr, "[AV1] Failed to open LK_DUMP_AV1 path: %s\n",
                 dump_path);
    std::fflush(stderr);
    return;
  }

  out.write(reinterpret_cast<const char*>(packet.data()),
            static_cast<std::streamsize>(packet.size()));
  dumped.store(true, std::memory_order_relaxed);
  std::fprintf(stderr, "[AV1] Dumped normalized access unit to %s (bytes=%zu, "
                       "keyframe=%d)\n",
               dump_path, packet.size(), keyframe ? 1 : 0);
  std::fflush(stderr);
}

}  // namespace

JetsonAV1EncoderImpl::JetsonAV1EncoderImpl(const webrtc::Environment& env,
                                           const SdpVideoFormat& format)
    : env_(env), encoder_(livekit::JetsonCodec::kAV1), format_(format) {}

JetsonAV1EncoderImpl::~JetsonAV1EncoderImpl() {
  Release();
}

void JetsonAV1EncoderImpl::ReportInit() {
  if (has_reported_init_) {
    return;
  }
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.AV1EncoderImpl.Event",
                            kAV1EncoderEventInit, kAV1EncoderEventMax);
  has_reported_init_ = true;
}

void JetsonAV1EncoderImpl::ReportError() {
  if (has_reported_error_) {
    return;
  }
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.AV1EncoderImpl.Event",
                            kAV1EncoderEventError, kAV1EncoderEventMax);
  has_reported_error_ = true;
}

int32_t JetsonAV1EncoderImpl::InitEncode(
    const VideoCodec* inst,
    const VideoEncoder::Settings& settings) {
  (void)settings;
  if (!inst || inst->codecType != kVideoCodecAV1) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->maxFramerate == 0) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->width < 1 || inst->height < 1) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->numberOfSimulcastStreams > 1) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  const auto scalability_mode = inst->GetScalabilityMode();
  if (scalability_mode.has_value() &&
      *scalability_mode != ScalabilityMode::kL1T1) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  int32_t release_ret = Release();
  if (release_ret != WEBRTC_VIDEO_CODEC_OK) {
    ReportError();
    return release_ret;
  }

  codec_ = *inst;
  sent_decodable_keyframe_ = false;
  cached_sequence_header_obu_.clear();
  // Reset the dependency-descriptor controller so the first encoded frame of
  // the new session is emitted as a keyframe with a fresh template structure.
  svc_controller_ = ScalableVideoControllerNoLayering();
  if (!codec_.GetScalabilityMode().has_value()) {
    codec_.SetScalabilityMode(ScalabilityMode::kL1T1);
  }

  if (codec_.numberOfSimulcastStreams == 0) {
    codec_.simulcastStream[0].width = codec_.width;
    codec_.simulcastStream[0].height = codec_.height;
  }

  const size_t new_capacity =
      CalcBufferSize(VideoType::kI420, codec_.width, codec_.height);
  encoded_image_.SetEncodedData(EncodedImageBuffer::Create(new_capacity));
  encoded_image_._encodedWidth = codec_.width;
  encoded_image_._encodedHeight = codec_.height;
  encoded_image_.set_size(0);

  configuration_.sending = false;
  configuration_.frame_dropping_on = codec_.GetFrameDropEnabled();
  configuration_.key_frame_interval =
      std::max<int>(1, static_cast<int>(codec_.maxFramerate)) * 2;

  configuration_.width = codec_.width;
  configuration_.height = codec_.height;

  configuration_.max_frame_rate = codec_.maxFramerate;
  configuration_.target_bps = codec_.startBitrate * 1000;
  configuration_.max_bps = codec_.maxBitrate * 1000;

  if (!encoder_.IsInitialized()) {
    if (!encoder_.Initialize(codec_.width, codec_.height, codec_.maxFramerate,
                             codec_.startBitrate * 1000,
                             configuration_.key_frame_interval)) {
      RTC_LOG(LS_ERROR) << "Failed to initialize Jetson MMAPI AV1 encoder.";
      ReportError();
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }

  ReportInit();

  SimulcastRateAllocator init_allocator(env_, codec_);
  VideoBitrateAllocation allocation =
      init_allocator.Allocate(VideoBitrateAllocationParameters(
          DataRate::KilobitsPerSec(codec_.startBitrate), codec_.maxFramerate));
  SetRates(RateControlParameters(allocation, codec_.maxFramerate));
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonAV1EncoderImpl::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
  encoded_image_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonAV1EncoderImpl::Release() {
  if (encoder_.IsInitialized()) {
    encoder_.Destroy();
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonAV1EncoderImpl::Encode(
    const VideoFrame& input_frame,
    const std::vector<VideoFrameType>* frame_types) {
  if (!encoder_.IsInitialized()) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!encoded_image_callback_) {
    RTC_LOG(LS_WARNING)
        << "InitEncode() has been called, but a callback function "
           "has not been set with RegisterEncodeCompleteCallback()";
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }

  bool is_keyframe_needed = false;
  if (configuration_.key_frame_request && configuration_.sending) {
    is_keyframe_needed = true;
  }
  if (frame_types && !frame_types->empty()) {
    if ((*frame_types)[0] == VideoFrameType::kVideoFrameKey) {
      is_keyframe_needed = true;
    }
    if ((*frame_types)[0] == VideoFrameType::kEmptyFrame) {
      return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
    }
  }

  if (!configuration_.sending) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  if (!sent_decodable_keyframe_) {
    is_keyframe_needed = true;
  }

  std::vector<uint8_t> packet;
  bool is_keyframe = false;

  auto* dmabuf = livekit::DmaBufVideoFrameBuffer::FromNative(
      input_frame.video_frame_buffer().get());
  if (dmabuf) {
    if (!encoder_.EncodeDmaBuf(dmabuf->dmabuf_fd(), is_keyframe_needed,
                               &packet, &is_keyframe)) {
      RTC_LOG(LS_ERROR)
          << "Failed to encode DmaBuf frame with Jetson MMAPI AV1.";
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  } else {
    webrtc::scoped_refptr<I420BufferInterface> frame_buffer =
        input_frame.video_frame_buffer()->ToI420();
    if (!frame_buffer) {
      RTC_LOG(LS_ERROR) << "Failed to convert frame to I420.";
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }

    RTC_DCHECK_EQ(configuration_.width, frame_buffer->width());
    RTC_DCHECK_EQ(configuration_.height, frame_buffer->height());

    if (!encoder_.Encode(frame_buffer->DataY(), frame_buffer->StrideY(),
                         frame_buffer->DataU(), frame_buffer->StrideU(),
                         frame_buffer->DataV(), frame_buffer->StrideV(),
                         is_keyframe_needed, &packet, &is_keyframe)) {
      RTC_LOG(LS_ERROR)
          << "Failed to encode frame with Jetson MMAPI AV1 encoder.";
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }

  if (packet.empty()) {
    RTC_LOG(LS_WARNING) << "Jetson MMAPI AV1 encoder returned empty packet; "
                           "skipping output.";
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  livekit::av1::StripIvfFrameHeaderIfPresent(&packet);
  if (packet.empty()) {
    RTC_LOG(LS_ERROR)
        << "Jetson MMAPI AV1 packet contained only IVF framing; skipping.";
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }
  livekit::av1::ConvertAnnexBToLowOverheadIfPresent(&packet);
  livekit::av1::StripNonTransferObusIfPresent(&packet);

  std::vector<uint8_t> sequence_header;
  if (livekit::av1::ExtractSequenceHeaderObu(packet.data(), packet.size(),
                                             &sequence_header)) {
    cached_sequence_header_obu_ = std::move(sequence_header);
  }

  const bool treat_as_keyframe = is_keyframe_needed || is_keyframe;
  if (treat_as_keyframe) {
    livekit::av1::EnsureSequenceHeaderOnKeyframe(&packet,
                                                   cached_sequence_header_obu_);
  }

  if (!livekit::av1::IsWebRtcParseable(packet.data(), packet.size())) {
    RTC_LOG(LS_ERROR)
        << "Jetson MMAPI AV1 bitstream is not parseable by WebRTC; "
           "dropping frame (size=" << packet.size() << ").";
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  DumpAv1PacketIfRequested(packet, treat_as_keyframe);

  if (treat_as_keyframe &&
      livekit::av1::HasSequenceHeaderObu(packet.data(), packet.size())) {
    sent_decodable_keyframe_ = true;
    configuration_.key_frame_request = false;
  } else if (!sent_decodable_keyframe_) {
    RTC_LOG(LS_WARNING)
        << "Jetson MMAPI AV1 keyframe still missing sequence header OBU; "
           "continuing to force keyframes.";
    configuration_.key_frame_request = true;
  } else if (is_keyframe_needed) {
    configuration_.key_frame_request = false;
  }

  return ProcessEncodedFrame(packet, input_frame, treat_as_keyframe);
}

int32_t JetsonAV1EncoderImpl::ProcessEncodedFrame(
    std::vector<uint8_t>& packet,
    const ::webrtc::VideoFrame& input_frame,
    bool is_keyframe) {
  encoded_image_._encodedWidth = codec_.width;
  encoded_image_._encodedHeight = codec_.height;
  encoded_image_.SetRtpTimestamp(input_frame.rtp_timestamp());
  encoded_image_.SetSimulcastIndex(0);
  encoded_image_.ntp_time_ms_ = input_frame.ntp_time_ms();
  encoded_image_.capture_time_ms_ = input_frame.render_time_ms();
  encoded_image_.rotation_ = input_frame.rotation();
  encoded_image_.content_type_ = VideoContentType::UNSPECIFIED;
  encoded_image_.timing_.flags = VideoSendTiming::kInvalid;
  encoded_image_._frameType =
      is_keyframe ? VideoFrameType::kVideoFrameKey
                  : VideoFrameType::kVideoFrameDelta;
  encoded_image_.SetColorSpace(input_frame.color_space());

  encoded_image_.SetEncodedData(
      EncodedImageBuffer::Create(packet.data(), packet.size()));
  encoded_image_.set_size(packet.size());

  encoded_image_.qp_ = -1;

  CodecSpecificInfo codecInfo;
  codecInfo.codecSpecific = {};
  codecInfo.codecType = kVideoCodecAV1;
  codecInfo.end_of_picture = true;
  codecInfo.scalability_mode = ScalabilityMode::kL1T1;

  // Attach the AV1 dependency-descriptor metadata. The hardware encoder only
  // produces an OBU bitstream, so we drive WebRTC's no-layering scalability
  // controller to generate the GenericFrameInfo for every frame (and the
  // FrameDependencyStructure on keyframes). This is required for the RTP layer
  // to packetize/send AV1 and for the SFU to forward it; without it the encoder
  // produces frames but no RTP packets are ever emitted. NextFrameConfig() is
  // called exactly once per emitted frame (after all drop checks) so the
  // dependency chain stays consistent with what is actually sent.
  std::vector<ScalableVideoController::LayerFrameConfig> layer_frames =
      svc_controller_.NextFrameConfig(/*restart=*/is_keyframe);
  if (!layer_frames.empty()) {
    const ScalableVideoController::LayerFrameConfig& layer_frame =
        layer_frames.front();
    codecInfo.generic_frame_info = svc_controller_.OnEncodeDone(layer_frame);
    if (layer_frame.IsKeyframe()) {
      codecInfo.template_structure = svc_controller_.DependencyStructure();
    }
  }

  const auto result =
      encoded_image_callback_->OnEncodedImage(encoded_image_, &codecInfo);
  if (result.error != EncodedImageCallback::Result::OK) {
    RTC_LOG(LS_ERROR) << "Encode m_encodedCompleteCallback failed "
                      << result.error;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

VideoEncoder::EncoderInfo JetsonAV1EncoderImpl::GetEncoderInfo() const {
  EncoderInfo info;
  info.supports_native_handle = true;
  info.implementation_name = "Jetson MMAPI AV1 Encoder";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  info.preferred_pixel_formats = {VideoFrameBuffer::Type::kNative,
                                  VideoFrameBuffer::Type::kI420};
  return info;
}

void JetsonAV1EncoderImpl::SetRates(const RateControlParameters& parameters) {
  if (!encoder_.IsInitialized()) {
    RTC_LOG(LS_WARNING) << "SetRates() while uninitialized.";
    return;
  }

  if (parameters.framerate_fps < 1.0) {
    RTC_LOG(LS_WARNING) << "Invalid frame rate: " << parameters.framerate_fps;
    return;
  }

  if (parameters.bitrate.get_sum_bps() == 0) {
    configuration_.SetStreamState(false);
    return;
  }

  codec_.maxFramerate = static_cast<uint32_t>(parameters.framerate_fps);
  codec_.maxBitrate = parameters.bitrate.GetSpatialLayerSum(0);

  configuration_.target_bps = parameters.bitrate.GetSpatialLayerSum(0);
  configuration_.max_frame_rate = parameters.framerate_fps;

  encoder_.SetRates(codec_.maxFramerate,
                    static_cast<int>(configuration_.target_bps));

  if (configuration_.target_bps) {
    configuration_.SetStreamState(true);
  } else {
    configuration_.SetStreamState(false);
  }
}

void JetsonAV1EncoderImpl::LayerConfig::SetStreamState(bool send_stream) {
  if (send_stream && !sending) {
    key_frame_request = true;
  }
  sending = send_stream;
}

}  // namespace webrtc
