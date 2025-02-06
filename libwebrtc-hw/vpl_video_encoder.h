#ifndef ANY_VPL_VIDEO_ENCODER_H_
#define ANY_VPL_VIDEO_ENCODER_H_

#include <memory>

// WebRTC
#include <api/video/video_codec_type.h>
#include <api/video_codecs/video_encoder.h>

#include "vpl_session.h"

namespace any_vpl {

/**
 * @brief An accelerated encoder using Intel® Video Processing Library (Intel® VPL), for accelerating encoding on Intel® hardware.
 *
 */
class VplVideoEncoder : public webrtc::VideoEncoder {
 public:
  static bool IsSupported(std::shared_ptr<VplSession> session, webrtc::VideoCodecType codec);
  static std::unique_ptr<VplVideoEncoder> Create(std::shared_ptr<VplSession> session, webrtc::VideoCodecType codec);
};

}  // namespace any_vpl

#endif
