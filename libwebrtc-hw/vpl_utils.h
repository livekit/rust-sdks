#ifndef ANY_VPL_UTILS_H_
#define ANY_VPL_UTILS_H_

#include <api/video/video_codec_type.h>
#include <mfxdefs.h>
#include <cstdlib>
#include <string>

namespace any_vpl {

#define ALIGN16(value) (((value + 15) >> 4) << 4)
#define ALIGN32(value) (((value + 31) >> 5) << 5)  // round up to a multiple of 32

mfxU32 ToMfxCodec(webrtc::VideoCodecType codec);

std::string CodecToString(mfxU32 codec);

}  // namespace any_vpl
#endif
