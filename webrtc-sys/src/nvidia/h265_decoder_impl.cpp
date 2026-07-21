#include "h265_decoder_impl.h"

#include <api/video/video_codec_type.h>
#include <modules/video_coding/include/video_error_codes.h>

#include "NvDecoder/NvDecoder.h"
#include "Utils/NvCodecUtils.h"
#include "nvidia_nv12_video_frame_buffer.h"
#include "rtc_base/checks.h"
#include "rtc_base/logging.h"

namespace webrtc {

static ColorSpace ExtractColorSpaceFromFormat(const CUVIDEOFORMAT& format) {
  return ColorSpace(
      static_cast<ColorSpace::PrimaryID>(
          format.video_signal_description.color_primaries),
      static_cast<ColorSpace::TransferID>(
          format.video_signal_description.transfer_characteristics),
      static_cast<ColorSpace::MatrixID>(
          format.video_signal_description.matrix_coefficients),
      static_cast<ColorSpace::RangeID>(
          format.video_signal_description.video_full_range_flag));
}

NvidiaH265DecoderImpl::NvidiaH265DecoderImpl(CUcontext context)
    : cu_context_(context),
      decoder_(nullptr),
      is_configured_decoder_(false),
      decoded_complete_callback_(nullptr) {}

NvidiaH265DecoderImpl::~NvidiaH265DecoderImpl() { Release(); }

VideoDecoder::DecoderInfo NvidiaH265DecoderImpl::GetDecoderInfo() const {
  VideoDecoder::DecoderInfo info;
  info.implementation_name = "NVIDIA H265 Decoder";
  info.is_hardware_accelerated = true;
  return info;
}

bool NvidiaH265DecoderImpl::Configure(const Settings& settings) {
  if (settings.codec_type() != kVideoCodecH265) {
    RTC_LOG(LS_ERROR) << "initialization failed: codec type is not H265";
    return false;
  }
  if (!settings.max_render_resolution().Valid()) {
    RTC_LOG(LS_ERROR)
        << "initialization failed on codec_settings width < 0 or height < 0";
    return false;
  }

  settings_ = settings;

  const CUresult result = cuCtxSetCurrent(cu_context_);
  if (!ck(result)) {
    RTC_LOG(LS_ERROR) << "initialization failed on cuCtxSetCurrent result"
                      << result;
    return false;
  }

  int maxWidth = 4096;
  int maxHeight = 4096;

  // WebRTC's real-time encoders produce I/P-only streams, so decode and
  // display order match and NVDEC can emit directly from its decode callback.
  decoder_ = std::make_shared<NvDecoder>(
      cu_context_, true, cudaVideoCodec_HEVC, true, true, nullptr, nullptr,
      false, maxWidth, maxHeight, 1000, true);
  return true;
}

int32_t NvidiaH265DecoderImpl::RegisterDecodeCompleteCallback(
    DecodedImageCallback* callback) {
  decoded_complete_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t NvidiaH265DecoderImpl::Release() {
  decoder_.reset();
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t NvidiaH265DecoderImpl::Decode(const EncodedImage& input_image,
                                      bool /*missing_frames*/,
                                      int64_t /*render_time_ms*/) {
  CUcontext current;
  if (!ck(cuCtxGetCurrent(&current))) {
    RTC_LOG(LS_ERROR) << "decode failed on cuCtxGetCurrent";
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (current != cu_context_) {
    RTC_LOG(LS_ERROR)
        << "decode failed: current context does not match held context";
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (decoded_complete_callback_ == nullptr) {
    RTC_LOG(LS_ERROR) << "decode failed: decoded_complete_callback_ not set";
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!input_image.data() || !input_image.size()) {
    RTC_LOG(LS_ERROR) << "decode failed: input image is null";
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  const int nFrameReturned = decoder_->Decode(
      input_image.data(), static_cast<int>(input_image.size()),
      CUVID_PKT_TIMESTAMP, input_image.RtpTimestamp());

  // A packet may only contain sequence metadata. Do not replay it: NVDEC will
  // emit the next complete picture from its decode callback.
  if (nFrameReturned == 0) {
    return WEBRTC_VIDEO_CODEC_OK;
  }

  is_configured_decoder_ = true;

  if (decoder_->GetOutputFormat() != cudaVideoSurfaceFormat_NV12) {
    RTC_LOG(LS_ERROR) << "not supported output format: "
                      << decoder_->GetOutputFormat();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  const ColorSpace& color_space =
      input_image.ColorSpace() ? *input_image.ColorSpace()
                               : ExtractColorSpaceFromFormat(
                                     decoder_->GetVideoFormatInfo());

  for (int i = 0; i < nFrameReturned; i++) {
    int64_t timeStamp;
    uint8_t* pFrame = decoder_->GetLockedFrame(&timeStamp);
    if (!pFrame) {
      RTC_LOG(LS_ERROR) << "NVDEC returned an empty locked H265 frame";
      return WEBRTC_VIDEO_CODEC_ERROR;
    }

    auto native_buffer = webrtc::make_ref_counted<
        livekit::NvidiaNv12VideoFrameBuffer>(
        decoder_, pFrame, cu_context_, decoder_->GetWidth(),
        decoder_->GetHeight(), decoder_->GetDeviceFramePitch());

    VideoFrame decoded_frame = VideoFrame::Builder()
                                   .set_video_frame_buffer(native_buffer)
                                   .set_timestamp_rtp(static_cast<uint32_t>(
                                       timeStamp))
                                   .set_color_space(color_space)
                                   .build();

    std::optional<int32_t> decodetime;
    std::optional<int> qp;  // Not parsed for H265 currently
    decoded_complete_callback_->Decoded(decoded_frame, decodetime, qp);
  }

  return WEBRTC_VIDEO_CODEC_OK;
}

}  // end namespace webrtc
