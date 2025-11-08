#include "h264_encoder_impl.h"

#include <optional>

#include "api/video/video_frame_buffer.h"
#include "api/video/codec_specific_info.h"
#include "api/video_codecs/h264_profile_level_id.h"
#include "modules/video_coding/codecs/h264/include/h264.h"
#include "livekit/video_frame_buffer.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "v4l2_h264_encoder.h"

namespace webrtc {

JetsonH264EncoderImpl::JetsonH264EncoderImpl(const SdpVideoFormat& format) : format_(format) {}
JetsonH264EncoderImpl::~JetsonH264EncoderImpl() { Release(); }

int32_t JetsonH264EncoderImpl::InitEncode(const VideoCodec* codec_settings, const Settings& /*settings*/) {
  if (!codec_settings || codec_settings->codecType != kVideoCodecH264) {
    RTC_LOG(LS_ERROR) << "JetsonH264EncoderImpl InitEncode: invalid codec";
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (codec_settings->width < 1 || codec_settings->height < 1 || codec_settings->maxFramerate == 0) {
    RTC_LOG(LS_ERROR) << "JetsonH264EncoderImpl InitEncode: invalid dimensions/framerate";
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  codec_ = codec_settings;
  sending_ = false;
  keyframe_requested_ = true;

  // Initialize V4L2 encoder
  v4l2_ = std::make_unique<livekit::V4L2H264Encoder>();
  int fps = std::max(1u, codec_->maxFramerate);
  int bitrate_bps = std::max(1, (int)codec_->startBitrate * 1000);
  if (!v4l2_->Initialize(codec_->width, codec_->height, (int)fps, bitrate_bps)) {
    RTC_LOG(LS_ERROR) << "JetsonH264EncoderImpl: failed to initialize V4L2 encoder";
    v4l2_.reset();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  RTC_LOG(LS_INFO) << "JetsonH264EncoderImpl initialized for " << codec_->width << "x" << codec_->height
                   << " @" << codec_->maxFramerate << "fps";
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH264EncoderImpl::RegisterEncodeCompleteCallback(EncodedImageCallback* callback) {
  encoded_image_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH264EncoderImpl::Release() {
  sending_ = false;
  codec_ = nullptr;
  encoded_image_callback_ = nullptr;
  if (v4l2_) {
    v4l2_->Shutdown();
    v4l2_.reset();
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH264EncoderImpl::Encode(const VideoFrame& frame, const std::vector<VideoFrameType>* frame_types) {
  if (!encoded_image_callback_) {
    RTC_LOG(LS_WARNING) << "JetsonH264EncoderImpl Encode called without callback";
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }

  if (frame_types && !frame_types->empty() && (*frame_types)[0] == VideoFrameType::kEmptyFrame) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  // Prefer native DMA-BUF frames for zero-copy.
  rtc::scoped_refptr<webrtc::VideoFrameBuffer> vfb = frame.video_frame_buffer();
  if (vfb->type() == webrtc::VideoFrameBuffer::Type::kNative) {
    livekit::DmabufNV12Info info{};
    if (livekit::vfb_is_dmabuf_nv12(vfb.get()) && livekit::vfb_get_dmabuf_nv12_info(vfb.get(), &info)) {
      if (!v4l2_) {
        RTC_LOG(LS_ERROR) << "JetsonH264EncoderImpl: V4L2 encoder not initialized";
        return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
      }
      livekit::DmabufPlanesNV12 planes{info.fd_y, info.fd_uv, info.width, info.height, info.stride_y, info.stride_uv};
      bool want_key = keyframe_requested_ || (frame_types && !frame_types->empty() &&
                                              (*frame_types)[0] == VideoFrameType::kVideoFrameKey);
      keyframe_requested_ = false;
      if (!v4l2_->EnqueueDmabufFrame(planes, want_key)) {
        RTC_LOG(LS_ERROR) << "JetsonH264EncoderImpl: failed to enqueue DMABUF frame";
        return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
      }
      // Try to dequeue encoded output (non-blocking)
      auto encoded = v4l2_->DequeueEncoded();
      if (!encoded.has_value() || encoded->empty()) {
        return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
      }

      // Wrap and deliver
      EncodedImage encoded_image;
      encoded_image.SetEncodedData(EncodedImageBuffer::Create(encoded->data(), encoded->size()));
      encoded_image._encodedWidth = codec_ ? codec_->width : info.width;
      encoded_image._encodedHeight = codec_ ? codec_->height : info.height;
      encoded_image.capture_time_ms_ = frame.render_time_ms();
      encoded_image.SetRtpTimestamp(frame.rtp_timestamp());
      encoded_image.ntp_time_ms_ = frame.ntp_time_ms();
      encoded_image.rotation_ = frame.rotation();
      encoded_image._frameType = VideoFrameType::kVideoFrameDelta;
      encoded_image.SetSimulcastIndex(0);

      CodecSpecificInfo codec_info;
      codec_info.codecType = kVideoCodecH264;
      codec_info.codecSpecific.H264.packetization_mode = H264PacketizationMode::NonInterleaved;
      auto res = encoded_image_callback_->OnEncodedImage(encoded_image, &codec_info);
      if (res.error != EncodedImageCallback::Result::OK) {
        RTC_LOG(LS_ERROR) << "JetsonH264EncoderImpl: callback error " << res.error;
        return WEBRTC_VIDEO_CODEC_ERROR;
      }
      return WEBRTC_VIDEO_CODEC_OK;
    }
    // If native but not our DMA-BUF, fall back.
  }

  // Fallback path (temporary): Drop frames until DMA-BUF path is wired.
  RTC_LOG(LS_WARNING) << "JetsonH264EncoderImpl: unsupported buffer type; expecting NV12 DMA-BUF native frames";
  return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
}

void JetsonH264EncoderImpl::SetRates(const RateControlParameters& parameters) {
  sending_ = parameters.bitrate.get_sum_bps() > 0 && parameters.framerate_fps >= 1.0;
  if (sending_ && !keyframe_requested_) {
    keyframe_requested_ = true;
  }
  if (v4l2_) {
    int fps = (int)std::max(1.0, parameters.framerate_fps);
    int bitrate_bps = (int)parameters.bitrate.get_sum_bps();
    v4l2_->UpdateRates(fps, bitrate_bps);
  }
}

VideoEncoder::EncoderInfo JetsonH264EncoderImpl::GetEncoderInfo() const {
  EncoderInfo info;
  info.implementation_name = "Jetson V4L2 H264 Encoder";
  info.supports_native_handle = true;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  return info;
}

}  // namespace webrtc


