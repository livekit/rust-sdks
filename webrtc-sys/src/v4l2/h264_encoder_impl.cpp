#include "v4l2/h264_encoder_impl.h"

#include <utility>

#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder_factory_template.h"
#if defined(WEBRTC_USE_H264)
#include "api/video_codecs/video_encoder_factory_template_open_h264_adapter.h"
#endif
#include "rtc_base/logging.h"

namespace webrtc {

namespace {

using SoftwareFactory = webrtc::VideoEncoderFactoryTemplate<
#if defined(WEBRTC_USE_H264)
    webrtc::OpenH264EncoderTemplateAdapter
#endif
    >;

}  // namespace

V4L2H264EncoderImpl::V4L2H264EncoderImpl(const webrtc::Environment& env,
                                         const SdpVideoFormat& format)
    : env_(env), format_(format) {
  // For now, delegate to the existing software H.264 encoder implementation.
  SoftwareFactory factory;
  auto original_format = webrtc::FuzzyMatchSdpVideoFormat(
      factory.GetSupportedFormats(), format_);
  if (!original_format) {
    RTC_LOG(LS_ERROR)
        << "V4L2H264EncoderImpl: requested format not supported by software "
           "factory, falling back to nullptr";
    return;
  }
  fallback_encoder_ = factory.Create(env_, *original_format);
}

V4L2H264EncoderImpl::~V4L2H264EncoderImpl() = default;

int32_t V4L2H264EncoderImpl::InitEncode(const VideoCodec* codec_settings,
                                        const Settings& settings) {
  if (!fallback_encoder_) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return fallback_encoder_->InitEncode(codec_settings, settings);
}

int32_t V4L2H264EncoderImpl::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
  if (!fallback_encoder_) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return fallback_encoder_->RegisterEncodeCompleteCallback(callback);
}

int32_t V4L2H264EncoderImpl::Release() {
  if (!fallback_encoder_) {
    return WEBRTC_VIDEO_CODEC_OK;
  }
  return fallback_encoder_->Release();
}

int32_t V4L2H264EncoderImpl::Encode(
    const VideoFrame& frame,
    const std::vector<VideoFrameType>* frame_types) {
  if (!fallback_encoder_) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return fallback_encoder_->Encode(frame, frame_types);
}

void V4L2H264EncoderImpl::SetRates(
    const RateControlParameters& rc_parameters) {
  if (!fallback_encoder_) {
    return;
  }
  fallback_encoder_->SetRates(rc_parameters);
}

EncoderInfo V4L2H264EncoderImpl::GetEncoderInfo() const {
  if (!fallback_encoder_) {
    EncoderInfo info;
    info.implementation_name = "V4L2-H264 (uninitialized)";
    return info;
  }
  EncoderInfo info = fallback_encoder_->GetEncoderInfo();
  // Override the implementation name to make it clear this goes through
  // the V4L2 wrapper (even though it's currently software-based).
  info.implementation_name = "V4L2-H264 (software fallback)";
  return info;
}

}  // namespace webrtc


