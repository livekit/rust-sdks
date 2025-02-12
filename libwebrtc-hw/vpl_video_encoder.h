#ifndef ANY_VPL_VIDEO_ENCODER_H_
#define ANY_VPL_VIDEO_ENCODER_H_

#include <memory>

// WebRTC
#include <api/video/video_codec_type.h>
#include <api/video_codecs/video_encoder.h>
#include <common_video/h264/h264_bitstream_parser.h>
#include <common_video/include/bitrate_adjuster.h>
#include "vpl_session_impl.h"

namespace any_vpl {

/**
 * @brief A class that implements an accelerated video encoder using Intel® VPL.
 *
 */
class VplVideoEncoder : public webrtc::VideoEncoder {
 public:
  VplVideoEncoder(std::shared_ptr<VplSession> session, webrtc::VideoCodecType codec);
  ~VplVideoEncoder() override;

  int32_t InitEncode(const webrtc::VideoCodec* codec_settings, int32_t number_of_cores, size_t max_payload_size) override;
  int32_t RegisterEncodeCompleteCallback(webrtc::EncodedImageCallback* callback) override;
  int32_t Release() override;
  int32_t Encode(const webrtc::VideoFrame& frame, const std::vector<webrtc::VideoFrameType>* frame_types) override;
  void SetRates(const RateControlParameters& parameters) override;
  webrtc::VideoEncoder::EncoderInfo GetEncoderInfo() const override;

 private:
  struct ExtBuffer {
    mfxExtBuffer* ext_buffers[10];
    mfxExtCodingOption ext_coding_option;
    mfxExtCodingOption2 ext_coding_option2;
  };

  std::mutex mutex_;
  webrtc::EncodedImageCallback* callback_ = nullptr;

  uint32_t targetBitrateBps_ = 0;
  uint32_t maxBitrateBps_ = 0;
  bool reconfigureNeeded_ = false;
  uint32_t width_ = 0;
  uint32_t height_ = 0;
  uint32_t framerate_ = 0;
  webrtc::VideoCodecMode mode_ = webrtc::VideoCodecMode::kRealtimeVideo;
  webrtc::EncodedImage encodedImage_;
  webrtc::H264BitstreamParser h264BitstreamParser_;

  std::vector<uint8_t> surfaceBuffer_;
  std::vector<mfxFrameSurface1> surfaces_;

  std::shared_ptr<VplSession> session_;
  mfxU32 codec_;
  webrtc::BitrateAdjuster bitrateAdjuster_;
  mfxFrameAllocRequest allocRequest_;
  std::unique_ptr<MFXVideoENCODE> encoder_;
  std::vector<uint8_t> bitstreamBuffer_;
  mfxBitstream bitstream_;
  mfxFrameInfo frameInfo_;

  int key_frame_interval_ = 0;

  mfxStatus ExecQuery(mfxVideoParam& param);

  /**
   * @brief Tries queries in various patterns and returns the param when successful
   *
   * @return mfxStatus
   */
  mfxStatus ExecQueries(mfxVideoParam& param, ExtBuffer& ext);

  /**
   * @brief Initialize VPL context
   *
   * @return int32_t WEBRTC_VIDEO_CODEC_OK if successful, otherwise an error code
   */
  int32_t InitVpl();

  /**
   * Close VPL context and release resources
   */
  int32_t ReleaseVpl();
};

}  // namespace any_vpl

#endif
