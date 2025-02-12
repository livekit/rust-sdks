#include "vpl_utils.h"

#include <mfxstructures.h>
#include <mfxvp8.h>
#include <rtc_base/logging.h>
#include <map>

namespace {

// std::array to declare as constexpr and avoid clang warnings about exit-time destructor
constexpr std::array<std::pair<mfxU32, const char*>, 5> CODEC_STRING_MAP = {{{MFX_CODEC_VP8, "MFX_CODEC_VP8"},
                                                                             {MFX_CODEC_VP9, "MFX_CODEC_VP9"},
                                                                             {MFX_CODEC_AV1, "MFX_CODEC_AV1"},
                                                                             {MFX_CODEC_AVC, "MFX_CODEC_AVC"},
                                                                             {MFX_CODEC_HEVC, "MFX_CODEC_HEVC"}}};

constexpr std::array<std::pair<webrtc::VideoCodecType, mfxU32>, 4> MFX_CODEC_MAP = {{{webrtc::kVideoCodecVP8, MFX_CODEC_VP8},
                                                                                     {webrtc::kVideoCodecVP9, MFX_CODEC_VP9},
                                                                                     {webrtc::kVideoCodecAV1, MFX_CODEC_AV1},
                                                                                     {webrtc::kVideoCodecH264, MFX_CODEC_AVC}}};

}  // namespace

namespace any_vpl {

mfxU32 ToMfxCodec(webrtc::VideoCodecType codec) {
  for (const auto& pair : MFX_CODEC_MAP) {
    if (pair.first == codec) {
      return pair.second;
    }
  }

  RTC_LOG(LS_ERROR) << __FUNCTION__ << "Unsupported codec: " << codec << " ... Defaulting to AVC";
  return static_cast<mfxU32>(MFX_CODEC_AVC);
}

std::string CodecToString(mfxU32 codec) {
  for (const auto& pair : CODEC_STRING_MAP) {
    if (pair.first == codec) {
      return pair.second;
    }
  }
  return "MFX_CODEC_UNKNOWN";
}

}  // namespace any_vpl