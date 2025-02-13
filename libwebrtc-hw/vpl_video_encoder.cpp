#include "vpl_video_encoder.h"

#include <chrono>
#include <mutex>

// WebRTC
#include <modules/video_coding/codecs/h264/include/h264.h>
#include <modules/video_coding/include/video_codec_interface.h>
#include <modules/video_coding/include/video_error_codes.h>
#include <rtc_base/logging.h>
#include <rtc_base/synchronization/mutex.h>

// Intel VPL
#include <fcntl.h>
#include <mfxdispatcher.h>
#include <mfxvideo++.h>
#include <mfxvideo.h>
#include <mfxvp8.h>
#include "va/va.h"
#include "va/va_drm.h"
// libyuv
#include <libyuv.h>

#include "vpl_utils.h"

namespace {

constexpr float MIN_ADJUSTED_BITRATE_PERCENTAGE = 0.5;
constexpr float MAX_ADJUSTED_BITRATE_PERCENTAGE = 0.95;

}  // namespace

namespace any_vpl {

VplVideoEncoder::VplVideoEncoder(webrtc::VideoCodecType codec)
    : codec_(ToMfxCodec(codec)), bitrateAdjuster_(MIN_ADJUSTED_BITRATE_PERCENTAGE, MAX_ADJUSTED_BITRATE_PERCENTAGE) {}

VplVideoEncoder::~VplVideoEncoder() {
  Release();
}

mfxStatus VplVideoEncoder::ExecQuery(mfxVideoParam& param) {
  mfxVideoParam queryParam;
  memcpy(&queryParam, &param, sizeof(param));
  mfxStatus mfxSts = encoder_->Query(&queryParam, &queryParam);

  if (mfxSts >= 0) {
#define PrintParamInfo(NAME) \
  if (param.NAME != queryParam.NAME) RTC_LOG(LS_WARNING) << "param " << #NAME << " old=" << param.NAME << " new=" << queryParam.NAME
    PrintParamInfo(mfx.LowPower);
    PrintParamInfo(mfx.BRCParamMultiplier);
    PrintParamInfo(mfx.FrameInfo.FrameRateExtN);
    PrintParamInfo(mfx.FrameInfo.FrameRateExtD);
    PrintParamInfo(mfx.FrameInfo.FourCC);
    PrintParamInfo(mfx.FrameInfo.ChromaFormat);
    PrintParamInfo(mfx.FrameInfo.PicStruct);
    PrintParamInfo(mfx.FrameInfo.CropX);
    PrintParamInfo(mfx.FrameInfo.CropY);
    PrintParamInfo(mfx.FrameInfo.CropW);
    PrintParamInfo(mfx.FrameInfo.CropH);
    PrintParamInfo(mfx.FrameInfo.Width);
    PrintParamInfo(mfx.FrameInfo.Height);
    PrintParamInfo(mfx.CodecId);
    PrintParamInfo(mfx.CodecProfile);
    PrintParamInfo(mfx.CodecLevel);
    PrintParamInfo(mfx.GopPicSize);
    PrintParamInfo(mfx.GopRefDist);
    PrintParamInfo(mfx.GopOptFlag);
    PrintParamInfo(mfx.IdrInterval);
    PrintParamInfo(mfx.TargetUsage);
    PrintParamInfo(mfx.RateControlMethod);
    PrintParamInfo(mfx.InitialDelayInKB);
    PrintParamInfo(mfx.TargetKbps);
    PrintParamInfo(mfx.MaxKbps);
    PrintParamInfo(mfx.BufferSizeInKB);
    PrintParamInfo(mfx.NumSlice);
    PrintParamInfo(mfx.NumRefFrame);
    PrintParamInfo(mfx.EncodedOrder);
    PrintParamInfo(mfx.DecodedOrder);
    PrintParamInfo(mfx.ExtendedPicStruct);
    PrintParamInfo(mfx.TimeStampCalc);
    PrintParamInfo(mfx.SliceGroupsPresent);
    PrintParamInfo(mfx.MaxDecFrameBuffering);
    PrintParamInfo(mfx.EnableReallocRequest);
    PrintParamInfo(AsyncDepth);
    PrintParamInfo(IOPattern);

    memcpy(&param, &queryParam, sizeof(param));
  }
  return mfxSts;
}

mfxStatus VplVideoEncoder::ExecQueries(mfxVideoParam& param, ExtBuffer& ext) {
  mfxStatus mfxSts = MFX_ERR_NONE;

  memset(&param, 0, sizeof(param));

  param.mfx.CodecId = codec_;

  // In case we need different configuration instead of the default, we can uncomment below options
  if (codec_ == MFX_CODEC_VP8) {
    // param.mfx.CodecProfile = MFX_PROFILE_VP8_0;
  } else if (codec_ == MFX_CODEC_VP9) {
    // param.mfx.CodecProfile = MFX_PROFILE_VP9_0;
  } else if (codec_ == MFX_CODEC_AVC) {
    // param.mfx.CodecProfile = MFX_PROFILE_AVC_HIGH;
    // param.mfx.CodecLevel = MFX_LEVEL_AVC_51;
    // param.mfx.CodecProfile = MFX_PROFILE_AVC_MAIN;
    // param.mfx.CodecLevel = MFX_LEVEL_AVC_1;
  } else if (codec_ == MFX_CODEC_HEVC) {
    RTC_LOG(LS_ERROR) << __FUNCTION__ << "Current version of WebRTC used by Livekit doesn't support h265";
    // param.mfx.CodecProfile = MFX_PROFILE_HEVC_MAIN;
    // param.mfx.CodecLevel = MFX_LEVEL_HEVC_1;
    // param.mfx.LowPower = MFX_CODINGOPTION_OFF;
  } else if (codec_ == MFX_CODEC_AV1) {
    // param.mfx.CodecProfile = MFX_PROFILE_AV1_MAIN;
  }

  param.mfx.TargetUsage = MFX_TARGETUSAGE_BALANCED;
  param.mfx.TargetKbps = bitrateAdjuster_.GetAdjustedBitrateBps() / 1000;
  param.mfx.MaxKbps = maxBitrateBps_ / 1000;
  param.mfx.RateControlMethod = MFX_RATECONTROL_VBR;
  param.mfx.FrameInfo.FrameRateExtN = framerate_;
  param.mfx.FrameInfo.FrameRateExtD = 1;
  param.mfx.FrameInfo.FourCC = MFX_FOURCC_NV12;
  param.mfx.FrameInfo.ChromaFormat = MFX_CHROMAFORMAT_YUV420;
  param.mfx.FrameInfo.PicStruct = MFX_PICSTRUCT_PROGRESSIVE;
  param.mfx.FrameInfo.CropX = 0;
  param.mfx.FrameInfo.CropY = 0;
  param.mfx.FrameInfo.CropW = width_;
  param.mfx.FrameInfo.CropH = height_;
  // Width must be a multiple of 16
  // Height must be a multiple of 16 in case of frame picture and a multiple of
  // 32 in case of field picture
  param.mfx.FrameInfo.Width = ALIGN16(width_);
  param.mfx.FrameInfo.Height = ALIGN16(height_);

  param.mfx.GopRefDist = 1;
  param.AsyncDepth = 1;
  param.IOPattern = MFX_IOPATTERN_IN_SYSTEM_MEMORY;

  mfxExtBuffer** extBuffers = ext.extBuffers;
  mfxExtCodingOption& extCodingOption = ext.extCodingOption;
  mfxExtCodingOption2& extCodingOption2 = ext.extCodingOption2;

  int extBuffersSize = 0;

  // In case we need extra configuration, we can uncomment below options
  if (codec_ == MFX_CODEC_AVC) {
    memset(&extCodingOption, 0, sizeof(extCodingOption));
    extCodingOption.Header.BufferId = MFX_EXTBUFF_CODING_OPTION;
    extCodingOption.Header.BufferSz = sizeof(extCodingOption);
    extCodingOption.AUDelimiter = MFX_CODINGOPTION_OFF;
    extCodingOption.MaxDecFrameBuffering = 1;
    // extCodingOption.NalHrdConformance = MFX_CODINGOPTION_OFF;
    // extCodingOption.VuiVclHrdParameters = MFX_CODINGOPTION_ON;
    // extCodingOption.SingleSeiNalUnit = MFX_CODINGOPTION_ON;
    // extCodingOption.RefPicMarkRep = MFX_CODINGOPTION_OFF;
    // extCodingOption.PicTimingSEI = MFX_CODINGOPTION_OFF;
    // extCodingOption.RecoveryPointSEI = MFX_CODINGOPTION_OFF;
    // extCodingOption.FramePicture = MFX_CODINGOPTION_OFF;
    // extCodingOption.FieldOutput = MFX_CODINGOPTION_ON;

    memset(&extCodingOption2, 0, sizeof(extCodingOption2));
    extCodingOption2.Header.BufferId = MFX_EXTBUFF_CODING_OPTION2;
    extCodingOption2.Header.BufferSz = sizeof(extCodingOption2);
    extCodingOption2.RepeatPPS = MFX_CODINGOPTION_ON;
    // extCodingOption2.MaxSliceSize = 1;
    // extCodingOption2.AdaptiveI = MFX_CODINGOPTION_ON;

    extBuffers[0] = (mfxExtBuffer*)&extCodingOption;
    extBuffers[1] = (mfxExtBuffer*)&extCodingOption2;
    extBuffersSize = 2;
  } else if (codec_ == MFX_CODEC_HEVC) {
    memset(&extCodingOption2, 0, sizeof(extCodingOption2));
    extCodingOption2.Header.BufferId = MFX_EXTBUFF_CODING_OPTION2;
    extCodingOption2.Header.BufferSz = sizeof(extCodingOption2);
    extCodingOption2.RepeatPPS = MFX_CODINGOPTION_ON;

    extBuffers[0] = (mfxExtBuffer*)&extCodingOption2;
    extBuffersSize = 1;
  }

  if (extBuffersSize != 0) {
    param.ExtParam = extBuffers;
    param.NumExtParam = extBuffersSize;
  }

  mfxSts = ExecQuery(param);
  if (mfxSts >= 0) {
    return mfxSts;
  }

  RTC_LOG(LS_WARNING) << "Unsupported encoder codec: codec=" << CodecToString(codec_) << " mfxSts=" << mfxSts
                      << " ... Retry with IOPattern IN_SYSTEM_MEMORY only";
  param.IOPattern = MFX_IOPATTERN_IN_SYSTEM_MEMORY;
  mfxSts = ExecQuery(param);
  if (mfxSts >= 0) {
    return mfxSts;
  }

  // Turn on LowPower and set H264/H265 to fixed QP mode
  RTC_LOG(LS_WARNING) << "Unsupported encoder codec: codec=" << CodecToString(codec_) << " mfxSts=" << mfxSts
                      << " ... Retry with low power mode";
  param.mfx.LowPower = MFX_CODINGOPTION_ON;
  if (codec_ == MFX_CODEC_AVC || codec_ == MFX_CODEC_HEVC) {
    param.mfx.RateControlMethod = MFX_RATECONTROL_CQP;
    param.mfx.QPI = 25;
    param.mfx.QPP = 33;
    param.mfx.QPB = 40;
  }
  mfxSts = ExecQuery(param);
  if (mfxSts >= 0) {
    return mfxSts;
  }

  RTC_LOG(LS_ERROR) << "Unsupported encoder codec: codec=" << CodecToString(codec_) << " mfxSts=" << mfxSts;

  return mfxSts;
}

int32_t VplVideoEncoder::InitEncode(const webrtc::VideoCodec* codecSettings, int32_t /*numberOfCores*/, size_t /*maxPayloadSize*/) {
  RTC_DCHECK(codecSettings);

  session_.reset();
  session_ = std::make_unique<VplSession>();
  if (!session_) {
    RTC_LOG(LS_ERROR) << "Failed to create VplSession";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  if (!session_->Initialize()) {
    RTC_LOG(LS_ERROR) << "Failed to initialize VplSession";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  Release();

  width_ = codecSettings->width;
  height_ = codecSettings->height;
  targetBitrateBps_ = codecSettings->startBitrate * 1000;
  maxBitrateBps_ = codecSettings->maxBitrate * 1000;
  bitrateAdjuster_.SetTargetBitrateBps(targetBitrateBps_);
  framerate_ = codecSettings->maxFramerate;
  mode_ = codecSettings->mode;

  RTC_LOG(LS_INFO) << "InitEncode " << targetBitrateBps_ << "bit/sec";

  // Initialize encoded image. Default buffer size: size of unencoded data.
  encodedImage_._encodedWidth = 0;
  encodedImage_._encodedHeight = 0;
  encodedImage_.set_size(0);
  encodedImage_.timing_.flags = webrtc::VideoSendTiming::TimingFrameFlags::kInvalid;
  encodedImage_.content_type_ = (codecSettings->mode == webrtc::VideoCodecMode::kScreensharing) ? webrtc::VideoContentType::SCREENSHARE
                                                                                                : webrtc::VideoContentType::UNSPECIFIED;

  return InitVpl();
}

int32_t VplVideoEncoder::RegisterEncodeCompleteCallback(webrtc::EncodedImageCallback* callback) {
  std::lock_guard<std::mutex> lock(mutex_);
  callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}
int32_t VplVideoEncoder::Release() {
  return ReleaseVpl();
}

int32_t VplVideoEncoder::Encode(const webrtc::VideoFrame& frame, const std::vector<webrtc::VideoFrameType>* frame_types) {
  bool send_key_frame = false;

  if (frame_types != nullptr) {
    // We only support a single stream.
    RTC_DCHECK_EQ(frame_types->size(), static_cast<size_t>(1));
    // Skip frame?
    if ((*frame_types)[0] == webrtc::VideoFrameType::kEmptyFrame) {
      return WEBRTC_VIDEO_CODEC_OK;
    }
    // Force key frame?
    send_key_frame = (*frame_types)[0] == webrtc::VideoFrameType::kVideoFrameKey;
  }

  // Remove unused input surfaces
  auto surface = std::find_if(surfaces_.begin(), surfaces_.end(), [](const mfxFrameSurface1& s) { return !s.Data.Locked; });
  if (surface == surfaces_.end()) {
    RTC_LOG(LS_ERROR) << "Surface not found";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // Convert I420 to NV12
  rtc::scoped_refptr<const webrtc::I420BufferInterface> frame_buffer = frame.video_frame_buffer()->ToI420();
  libyuv::I420ToNV12(frame_buffer->DataY(), frame_buffer->StrideY(), frame_buffer->DataU(), frame_buffer->StrideU(), frame_buffer->DataV(),
                     frame_buffer->StrideV(), surface->Data.Y, surface->Data.Pitch, surface->Data.U, surface->Data.Pitch,
                     frame_buffer->width(), frame_buffer->height());

  mfxStatus mfxSts;

  mfxEncodeCtrl ctrl;
  memset(&ctrl, 0, sizeof(ctrl));
  if (send_key_frame) {
    ctrl.FrameType = MFX_FRAMETYPE_I | MFX_FRAMETYPE_IDR | MFX_FRAMETYPE_REF;
  } else {
    ctrl.FrameType = MFX_FRAMETYPE_UNKNOWN;
  }

  if (reconfigureNeeded_) {
    const auto start_time = std::chrono::system_clock::now();
    RTC_LOG(LS_INFO) << "Start reconfigure: bps=" << (bitrateAdjuster_.GetAdjustedBitrateBps() / 1000) << " framerate=" << framerate_;

    mfxVideoParam param;
    memset(&param, 0, sizeof(param));
    mfxSts = encoder_->GetVideoParam(&param);
    if (mfxSts < MFX_ERR_NONE) {
      RTC_LOG(LS_ERROR) << "GetVideoParam failed: mfxSts=" << mfxSts;
      return WEBRTC_VIDEO_CODEC_ERROR;
    }

    param.mfx.TargetKbps = bitrateAdjuster_.GetAdjustedBitrateBps() / 1000;
    param.mfx.FrameInfo.FrameRateExtN = framerate_;

    mfxSts = encoder_->Reset(&param);
    if (mfxSts < MFX_ERR_NONE) {
      RTC_LOG(LS_ERROR) << "Encoder Reset failed: mfxSts=" << mfxSts;
      return WEBRTC_VIDEO_CODEC_ERROR;
    }

    reconfigureNeeded_ = false;

    const auto end_time = std::chrono::system_clock::now();
    RTC_LOG(LS_INFO) << "Finish reconfigure: " << std::chrono::duration_cast<std::chrono::milliseconds>(end_time - start_time).count()
                     << " ms";
  }

  mfxSyncPoint syncp;
  mfxSts = encoder_->EncodeFrameAsync(&ctrl, &*surface, &bitstream_, &syncp);
  if (mfxSts == MFX_ERR_MORE_DATA) {
    // More input needed, try again
    return WEBRTC_VIDEO_CODEC_OK;
  }
  if (mfxSts < MFX_ERR_NONE) {
    RTC_LOG(LS_ERROR) << "EncodeFrameAsync failed: mfxSts=" << mfxSts;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  static constexpr mfxU32 waitMs = 300000;
  mfxSts = MFXVideoCORE_SyncOperation(session_->GetSession(), syncp, waitMs);
  if (mfxSts < MFX_ERR_NONE) {
    RTC_LOG(LS_ERROR) << "MFXVideoCORE_SyncOperation failed: mfxSts=" << mfxSts;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  uint8_t* p = bitstream_.Data + bitstream_.DataOffset;
  int size = bitstream_.DataLength;
  bitstream_.DataLength = 0;

  auto buf = webrtc::EncodedImageBuffer::Create(p, size);
  encodedImage_.SetEncodedData(buf);
  encodedImage_._encodedWidth = width_;
  encodedImage_._encodedHeight = height_;
  encodedImage_.content_type_ =
      (mode_ == webrtc::VideoCodecMode::kScreensharing) ? webrtc::VideoContentType::SCREENSHARE : webrtc::VideoContentType::UNSPECIFIED;
  encodedImage_.timing_.flags = webrtc::VideoSendTiming::kInvalid;
  encodedImage_.SetTimestamp(frame.timestamp());
  encodedImage_.ntp_time_ms_ = frame.ntp_time_ms();
  encodedImage_.capture_time_ms_ = frame.render_time_ms();
  encodedImage_.rotation_ = frame.rotation();
  encodedImage_.SetColorSpace(frame.color_space());
  keyFrameInterval_ += 1;
  if (bitstream_.FrameType & MFX_FRAMETYPE_I || bitstream_.FrameType & MFX_FRAMETYPE_IDR) {
    encodedImage_._frameType = webrtc::VideoFrameType::kVideoFrameKey;
    RTC_LOG(LS_INFO) << "Key Frame Generated: key_frame_interval=" << keyFrameInterval_;
    keyFrameInterval_ = 0;
  } else {
    encodedImage_._frameType = webrtc::VideoFrameType::kVideoFrameDelta;
  }

  webrtc::CodecSpecificInfo codecSpecific;
  if (codec_ == MFX_CODEC_AVC) {
    codecSpecific.codecType = webrtc::kVideoCodecH264;
    codecSpecific.codecSpecific.H264.packetization_mode = webrtc::H264PacketizationMode::NonInterleaved;

    h264BitstreamParser_.ParseBitstream(encodedImage_);
    encodedImage_.qp_ = h264BitstreamParser_.GetLastSliceQp().value_or(-1);
  }

  std::unique_lock<std::mutex> lock(mutex_);
  webrtc::EncodedImageCallback::Result result = callback_->OnEncodedImage(encodedImage_, &codecSpecific);
  lock.unlock();

  if (result.error != webrtc::EncodedImageCallback::Result::OK) {
    RTC_LOG(LS_ERROR) << __FUNCTION__ << " OnEncodedImage failed error:" << result.error;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  bitrateAdjuster_.Update(size);

  return WEBRTC_VIDEO_CODEC_OK;
}

void VplVideoEncoder::SetRates(const RateControlParameters& parameters) {
  if (parameters.framerate_fps < 1.0) {
    RTC_LOG(LS_WARNING) << "Invalid frame rate: " << parameters.framerate_fps;
    return;
  }

  uint32_t newFramerate = (uint32_t)parameters.framerate_fps;
  uint32_t newBitrate = parameters.bitrate.get_sum_bps();
  RTC_LOG(LS_INFO) << __FUNCTION__ << " framerate_:" << framerate_ << " newFramerate: " << newFramerate
                   << " targetBitrateBps_:" << targetBitrateBps_ << " newBitrate:" << newBitrate << " maxBitrateBps_:" << maxBitrateBps_;
  framerate_ = newFramerate;
  targetBitrateBps_ = newBitrate;
  bitrateAdjuster_.SetTargetBitrateBps(targetBitrateBps_);
  reconfigureNeeded_ = true;
}

webrtc::VideoEncoder::EncoderInfo VplVideoEncoder::GetEncoderInfo() const {
  webrtc::VideoEncoder::EncoderInfo info;
  info.supports_native_handle = true;
  info.implementation_name = "libvpl";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  return info;
}

int32_t VplVideoEncoder::InitVpl() {
  encoder_ = std::make_unique<MFXVideoENCODE>(session_->GetSession());

  mfxPlatform platform;
  memset(&platform, 0, sizeof(platform));
  MFXVideoCORE_QueryPlatform(session_->GetSession(), &platform);
  RTC_LOG(LS_INFO) << "Codec=" << CodecToString(codec_) << " CodeName=" << platform.CodeName << " DeviceId=" << platform.DeviceId
                   << " MediaAdapterType=" << platform.MediaAdapterType;

  mfxVideoParam param;
  // ExtBuffer needs to be declared here so that it is not outlived by the param, by the end of this function
  ExtBuffer extendedBufferOpts;
  mfxStatus mfxSts = ExecQueries(param, extendedBufferOpts);
  if (mfxSts < MFX_ERR_NONE) {
    RTC_LOG(LS_ERROR) << "Failed ExecQueries: mfxSts=" << mfxSts;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  if (mfxSts > MFX_ERR_NONE) {
    RTC_LOG(LS_WARNING) << "Supported specified codec but has warning: codec=" << CodecToString(codec_) << " mfxSts=" << mfxSts;
  }

  mfxSts = encoder_->Init(&param);
  if (mfxSts < MFX_ERR_NONE) {
    RTC_LOG(LS_ERROR) << "Failed to Init encoder: mfxSts=" << mfxSts;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  mfxSts = MFX_ERR_NONE;
  memset(&param, 0, sizeof(param));

  // Retrieve video parameters selected by encoder.
  // - BufferSizeInKB parameter is required to set bit stream buffer size
  mfxSts = encoder_->GetVideoParam(&param);
  if (mfxSts < MFX_ERR_NONE) {
    RTC_LOG(LS_ERROR) << "Failed to GetVideoParam: mfxSts=" << mfxSts;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  RTC_LOG(LS_INFO) << "BufferSizeInKB=" << param.mfx.BufferSizeInKB;

  // Query number of required surfaces for encoder
  memset(&allocRequest_, 0, sizeof(allocRequest_));
  mfxSts = encoder_->QueryIOSurf(&param, &allocRequest_);
  if (mfxSts < MFX_ERR_NONE) {
    RTC_LOG(LS_ERROR) << "Failed to QueryIOSurf: mfxSts=" << mfxSts;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  RTC_LOG(LS_INFO) << "Encoder NumFrameSuggested=" << allocRequest_.NumFrameSuggested;

  frameInfo_ = param.mfx.FrameInfo;

  // Initializing the Output Bitstream
  bitstreamBuffer_.resize(param.mfx.BufferSizeInKB * 1000);

  memset(&bitstream_, 0, sizeof(bitstream_));
  bitstream_.MaxLength = bitstreamBuffer_.size();
  bitstream_.Data = bitstreamBuffer_.data();

  const int width = ALIGN32(allocRequest_.Info.Width);
  const int height = ALIGN32(allocRequest_.Info.Height);
  // Number of bytes per page
  // NV12 => 12 bits per pixel
  const int size = width * height * 12 / 8;
  surfaceBuffer_.resize(allocRequest_.NumFrameSuggested * size);

  surfaces_.clear();
  surfaces_.reserve(allocRequest_.NumFrameSuggested);
  for (int i = 0; i < allocRequest_.NumFrameSuggested; i++) {
    mfxFrameSurface1 surface;
    memset(&surface, 0, sizeof(surface));
    surface.Info = frameInfo_;
    surface.Data.Y = surfaceBuffer_.data() + i * size;
    surface.Data.U = surfaceBuffer_.data() + i * size + width * height;
    surface.Data.V = surfaceBuffer_.data() + i * size + width * height + 1;
    surface.Data.Pitch = width;
    surfaces_.push_back(surface);
  }

  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t VplVideoEncoder::ReleaseVpl() {
  if (encoder_ != nullptr) {
    encoder_->Close();
  }
  encoder_.reset();
  return WEBRTC_VIDEO_CODEC_OK;
}

}  // namespace any_vpl
