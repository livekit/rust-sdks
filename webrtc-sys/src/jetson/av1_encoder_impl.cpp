#include "av1_encoder_impl.h"

#include <algorithm>
#include <cstdio>
#include <limits>
#include <string>

#include "absl/strings/match.h"
#include "absl/types/optional.h"
#include "api/video/video_codec_constants.h"
#include "api/video_codecs/scalability_mode.h"
#include "common_video/libyuv/include/webrtc_libyuv.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "modules/video_coding/svc/create_scalability_structure.h"
#include "modules/video_coding/utility/simulcast_rate_allocator.h"
#include "modules/video_coding/utility/simulcast_utility.h"
#include "rtc_base/checks.h"
#include "rtc_base/logging.h"
#include "rtc_base/time_utils.h"
#include "system_wrappers/include/metrics.h"
#include "third_party/libyuv/include/libyuv/convert.h"

namespace webrtc {

enum AV1EncoderImplEvent {
  kAV1EncoderEventInit = 0,
  kAV1EncoderEventError = 1,
  kAV1EncoderEventMax = 16,
};

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
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.JetsonAV1EncoderImpl.Event",
                            kAV1EncoderEventInit, kAV1EncoderEventMax);
  has_reported_init_ = true;
}

void JetsonAV1EncoderImpl::ReportError() {
  if (has_reported_error_) {
    return;
  }
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.JetsonAV1EncoderImpl.Event",
                            kAV1EncoderEventError, kAV1EncoderEventMax);
  has_reported_error_ = true;
}

int32_t JetsonAV1EncoderImpl::InitEncode(
    const VideoCodec* inst,
    const VideoEncoder::Settings& settings) {
  (void)settings;

  std::fprintf(stderr,
               "[AV1Impl] InitEncode() called: %dx%d @ %d fps, "
               "startBitrate=%d kbps, maxBitrate=%d kbps\n",
               inst ? inst->width : 0, inst ? inst->height : 0,
               inst ? inst->maxFramerate : 0,
               inst ? inst->startBitrate : 0,
               inst ? inst->maxBitrate : 0);
  std::fflush(stderr);

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

  int32_t release_ret = Release();
  if (release_ret != WEBRTC_VIDEO_CODEC_OK) {
    ReportError();
    return release_ret;
  }

  codec_ = *inst;

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
  configuration_.key_frame_interval = 0;

  configuration_.width = codec_.width;
  configuration_.height = codec_.height;

  configuration_.max_frame_rate = codec_.maxFramerate;
  configuration_.target_bps = codec_.startBitrate * 1000;
  configuration_.max_bps = codec_.maxBitrate * 1000;

  if (!encoder_.IsInitialized()) {
    int key_frame_interval = codec_.maxFramerate * 5;
    if (!encoder_.Initialize(codec_.width, codec_.height, codec_.maxFramerate,
                             codec_.startBitrate * 1000, key_frame_interval)) {
      RTC_LOG(LS_ERROR) << "Failed to initialize Jetson MMAPI AV1 encoder.";
      ReportError();
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }

  std::fprintf(stderr,
               "[AV1Impl] InitEncode() completed: encoder %dx%d, "
               "config %dx%d\n",
               codec_.width, codec_.height,
               configuration_.width, configuration_.height);
  std::fflush(stderr);

  svc_controller_ = CreateScalabilityStructure(ScalabilityMode::kL1T1);
  if (!svc_controller_) {
    RTC_LOG(LS_ERROR) << "Failed to create L1T1 scalability controller";
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERROR;
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
  ivf_header_detected_ = false;
  svc_controller_.reset();
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonAV1EncoderImpl::Encode(
    const VideoFrame& input_frame,
    const std::vector<VideoFrameType>* frame_types) {
  static uint64_t encode_count = 0;
  const uint64_t frame_num = encode_count++;

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

  webrtc::scoped_refptr<I420BufferInterface> frame_buffer =
      input_frame.video_frame_buffer()->ToI420();
  if (!frame_buffer) {
    RTC_LOG(LS_ERROR) << "Failed to convert frame to I420.";
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  if (frame_num < 3) {
    std::fprintf(stderr,
                 "[AV1Impl] Encode() frame %lu: input=%dx%d, config=%dx%d, "
                 "keyframe_needed=%d, sending=%d\n",
                 frame_num, frame_buffer->width(), frame_buffer->height(),
                 configuration_.width, configuration_.height,
                 is_keyframe_needed ? 1 : 0,
                 configuration_.sending ? 1 : 0);
    std::fflush(stderr);
  }

  RTC_DCHECK_EQ(configuration_.width, frame_buffer->width());
  RTC_DCHECK_EQ(configuration_.height, frame_buffer->height());

  std::vector<uint8_t> packet;
  bool is_keyframe = false;
  if (!encoder_.Encode(frame_buffer->DataY(), frame_buffer->StrideY(),
                       frame_buffer->DataU(), frame_buffer->StrideU(),
                       frame_buffer->DataV(), frame_buffer->StrideV(),
                       is_keyframe_needed, &packet, &is_keyframe)) {
    RTC_LOG(LS_ERROR) << "Failed to encode frame with Jetson MMAPI AV1 encoder.";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  if (packet.empty()) {
    RTC_LOG(LS_WARNING) << "Jetson MMAPI AV1 encoder returned empty packet; "
                           "skipping output.";
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  // The Jetson MMAPI AV1 encoder wraps output in an IVF container.
  // WebRTC's AV1 RTP packetizer expects raw OBU data, so strip the IVF
  // headers before handing off to the packetizer.
  //
  // First frame: 32-byte IVF file header + 12-byte IVF frame header = 44 bytes
  // Subsequent:  12-byte IVF frame header
  {
    size_t strip = 0;
    if (packet.size() >= 32 + 12 &&
        packet[0] == 'D' && packet[1] == 'K' &&
        packet[2] == 'I' && packet[3] == 'F') {
      uint16_t hdr_len = packet[6] | (static_cast<uint16_t>(packet[7]) << 8);
      strip = static_cast<size_t>(hdr_len) + 12;
      if (frame_num < 3) {
        std::fprintf(stderr,
                     "[AV1Impl] Stripping IVF file header (%u bytes) + "
                     "frame header (12 bytes) = %zu bytes from %zu byte packet\n",
                     hdr_len, strip, packet.size());
        std::fflush(stderr);
      }
    } else if (packet.size() >= 12 + 2 && ivf_header_detected_) {
      // IVF frame header: 4 bytes size (LE) + 8 bytes timestamp
      uint32_t ivf_frame_size = packet[0] |
          (static_cast<uint32_t>(packet[1]) << 8) |
          (static_cast<uint32_t>(packet[2]) << 16) |
          (static_cast<uint32_t>(packet[3]) << 24);
      if (ivf_frame_size + 12 == packet.size() ||
          ivf_frame_size + 12 <= packet.size()) {
        strip = 12;
        if (frame_num < 5) {
          std::fprintf(stderr,
                       "[AV1Impl] Stripping IVF frame header (12 bytes) "
                       "from %zu byte packet (ivf_frame_size=%u)\n",
                       packet.size(), ivf_frame_size);
          std::fflush(stderr);
        }
      }
    }
    if (strip > 0) {
      ivf_header_detected_ = true;
      if (strip >= packet.size()) {
        RTC_LOG(LS_WARNING) << "IVF strip offset exceeds packet size";
        return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
      }
      packet.erase(packet.begin(), packet.begin() + strip);
    }
  }

  // Detect keyframes from the OBU stream: presence of a Sequence Header OBU
  // (type 1) indicates a keyframe. This is more reliable than
  // V4L2_BUF_FLAG_KEYFRAME which Jetson's AV1 encoder may not set.
  bool obu_detected_keyframe = false;
  {
    size_t pos = 0;
    int obu_count = 0;
    while (pos < packet.size() && obu_count < 16) {
      uint8_t hdr = packet[pos];
      int obu_type = (hdr >> 3) & 0xF;
      bool has_extension = (hdr >> 2) & 1;
      bool has_size = (hdr >> 1) & 1;

      if (obu_type == 1) {
        obu_detected_keyframe = true;
      }

      if (frame_num < 5) {
        const char* type_name = "unknown";
        switch (obu_type) {
          case 1: type_name = "SEQ_HDR"; break;
          case 2: type_name = "TD"; break;
          case 3: type_name = "FRAME_HDR"; break;
          case 4: type_name = "TILE_GROUP"; break;
          case 5: type_name = "METADATA"; break;
          case 6: type_name = "FRAME"; break;
          case 7: type_name = "REDUNDANT_FRAME_HDR"; break;
          case 8: type_name = "TILE_LIST"; break;
          case 15: type_name = "PADDING"; break;
        }
        std::fprintf(stderr,
                     "[AV1Impl]   OBU[%d] @%zu: type=%d(%s) ext=%d has_size=%d",
                     obu_count, pos, obu_type, type_name,
                     has_extension ? 1 : 0, has_size ? 1 : 0);
      }

      pos += 1;
      if (has_extension && pos < packet.size()) pos += 1;
      if (has_size && pos < packet.size()) {
        uint64_t obu_size = 0;
        int shift = 0;
        while (pos < packet.size()) {
          uint8_t byte = packet[pos++];
          obu_size |= (uint64_t)(byte & 0x7F) << shift;
          shift += 7;
          if (!(byte & 0x80)) break;
        }
        if (frame_num < 5) {
          std::fprintf(stderr, " size=%lu", (unsigned long)obu_size);
        }
        pos += obu_size;
      } else if (!has_size) {
        if (frame_num < 5) {
          std::fprintf(stderr, " (NO SIZE FIELD - rest of bitstream is this OBU)");
        }
        pos = packet.size();
      }
      if (frame_num < 5) {
        std::fprintf(stderr, "\n");
      }
      obu_count++;
    }
  }

  if (obu_detected_keyframe && !is_keyframe) {
    is_keyframe = true;
    if (frame_num < 10) {
      std::fprintf(stderr,
                   "[AV1Impl] frame %lu: V4L2 did not flag keyframe but "
                   "OBU stream contains SEQ_HDR -> overriding to keyframe\n",
                   frame_num);
    }
  }

  if (frame_num < 5 && packet.size() >= 2) {
    std::fprintf(stderr,
                 "[AV1Impl] Encode() frame %lu: encoded %zu bytes, "
                 "keyframe=%d (obu_detected=%d), first_bytes=[",
                 frame_num, packet.size(), is_keyframe ? 1 : 0,
                 obu_detected_keyframe ? 1 : 0);
    for (size_t b = 0; b < std::min(packet.size(), (size_t)16); ++b)
      std::fprintf(stderr, "%02x ", packet[b]);
    std::fprintf(stderr, "]\n");
    std::fflush(stderr);
  }

  if (is_keyframe_needed) {
    configuration_.key_frame_request = false;
  }

  return ProcessEncodedFrame(packet, input_frame, is_keyframe);
}

int32_t JetsonAV1EncoderImpl::ProcessEncodedFrame(
    std::vector<uint8_t>& packet,
    const ::webrtc::VideoFrame& input_frame,
    bool is_keyframe) {
  static uint64_t process_count = 0;
  const uint64_t frame_num = process_count++;

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
  codecInfo.codecType = kVideoCodecAV1;

  // Generate dependency descriptor via the L1T1 scalability controller.
  // The SFU requires this RTP header extension to forward AV1 packets.
  if (svc_controller_) {
    auto configs = svc_controller_->NextFrameConfig(is_keyframe);
    if (!configs.empty()) {
      auto& cfg = configs[0];
      codecInfo.generic_frame_info =
          svc_controller_->OnEncodeDone(std::move(cfg));
    }
    if (is_keyframe) {
      codecInfo.template_structure = svc_controller_->DependencyStructure();
    }
    codecInfo.scalability_mode = ScalabilityMode::kL1T1;
  }

  const auto result =
      encoded_image_callback_->OnEncodedImage(encoded_image_, &codecInfo);
  if (result.error != EncodedImageCallback::Result::OK) {
    RTC_LOG(LS_ERROR) << "Encode m_encodedCompleteCallback failed "
                      << result.error;
    std::fprintf(stderr,
                 "[AV1Impl] OnEncodedImage FAILED: error=%d, frame=%lu, "
                 "size=%zu, declared=%dx%d\n",
                 static_cast<int>(result.error), frame_num, packet.size(),
                 codec_.width, codec_.height);
    std::fflush(stderr);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  if (frame_num < 10 || (frame_num % 100 == 0)) {
    std::fprintf(stderr,
                 "[AV1Impl] OnEncodedImage OK: frame=%lu, size=%zu, "
                 "declared=%dx%d, keyframe=%d\n",
                 frame_num, packet.size(),
                 codec_.width, codec_.height, is_keyframe ? 1 : 0);
    std::fflush(stderr);
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

VideoEncoder::EncoderInfo JetsonAV1EncoderImpl::GetEncoderInfo() const {
  EncoderInfo info;
  info.supports_native_handle = false;
  info.implementation_name = "Jetson MMAPI AV1 Encoder";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  info.preferred_pixel_formats = {VideoFrameBuffer::Type::kI420};
  return info;
}

void JetsonAV1EncoderImpl::SetRates(
    const RateControlParameters& parameters) {
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
  if (send_stream != sending) {
    std::fprintf(stderr, "[AV1Impl] SetStreamState: sending %d -> %d\n",
                 sending ? 1 : 0, send_stream ? 1 : 0);
    std::fflush(stderr);
  }
  if (send_stream && !sending) {
    key_frame_request = true;
  }
  sending = send_stream;
}

}  // namespace webrtc
