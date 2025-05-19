#include "h264_encoder_impl.h"

#include <algorithm>
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
#include "third_party/libyuv/include/libyuv/scale.h"

namespace webrtc {

VAAPIH264EncoderWrapper::VAAPIH264EncoderWrapper(
    std::unique_ptr<livekit::VaapiEncoderWrapper> vaapi_encoder)
    : encoder_(std::move(vaapi_encoder)) {
  encoder_info_.is_hardware_accelerated = true;
}

VAAPIH264EncoderWrapper::~VAAPIH264EncoderWrapper() {
  Release();
}

int32_t VAAPIH264EncoderWrapper::InitEncode(
    const VideoCodec* inst,
    const VideoEncoder::Settings& settings) {
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t VAAPIH264EncoderWrapper::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
  RTC_DCHECK(callback);
  callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t VAAPIH264EncoderWrapper::Release() {
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t VAAPIH264EncoderWrapper::Encode(
    const VideoFrame& frame,
    const std::vector<VideoFrameType>* frame_types) {
  RTC_DCHECK(frame.video_frame_buffer()->type() ==
             VideoFrameBuffer::Type::kI420);

  return WEBRTC_VIDEO_CODEC_OK;
}

void VAAPIH264EncoderWrapper::SetRates(
    const RateControlParameters& rc_parameters) {}

}  // namespace webrtc
