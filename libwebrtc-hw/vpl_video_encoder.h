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
  /**
   * @brief Construct a new Vpl Video Encoder object
   *
   * @param session A Vpl session
   * @param codec The codec to use
   */
  VplVideoEncoder(std::shared_ptr<VplSession> session, webrtc::VideoCodecType codec);
  virtual ~VplVideoEncoder() override;

  // webrtc::VideoEncoder overrides
  int32_t InitEncode(const webrtc::VideoCodec* codecSettings, int32_t numberOfCores, size_t maxPayloadSize) override;
  int32_t RegisterEncodeCompleteCallback(webrtc::EncodedImageCallback* callback) override;
  int32_t Release() override;
  int32_t Encode(const webrtc::VideoFrame& frame, const std::vector<webrtc::VideoFrameType>* frameTypes) override;
  void SetRates(const RateControlParameters& parameters) override;
  webrtc::VideoEncoder::EncoderInfo GetEncoderInfo() const override;

 private:
  /**
   * @brief Struct that aggregates variables for setting additional options for encoding
   *
   */
  struct ExtBuffer {
    mfxExtBuffer* extBuffers[2];
    mfxExtCodingOption extCodingOption;
    mfxExtCodingOption2 extCodingOption2;
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

  int keyFrameInterval_ = 0;

  /**
   * @brief Helper method to execute a query
   *
   * @param param Configuration parameters for encoding
   * @return mfxStatus MFX_ERR_NONE if successful, otherwise an error/warning code
   */
  mfxStatus ExecQuery(mfxVideoParam& param);

  /**
   * @brief Tries queries in various patterns and returns the param when successful
   *
   * @param param Configuration parameters for encoding
   * @param ext Additional options for encoding
   * @return mfxStatus MFX_ERR_NONE if successful, otherwise an error/warning code
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
