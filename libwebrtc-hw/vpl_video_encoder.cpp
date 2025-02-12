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

#include "vpl_session_impl.h"
#include "vpl_utils.h"

namespace {
constexpr int kLowH264QpThreshold = 34;
constexpr int kHighH264QpThreshold = 40;
constexpr float MIN_ADJUSTED_BITRATE_PERCENTAGE = 0.5;
constexpr float MAX_ADJUSTED_BITRATE_PERCENTAGE = 0.95;

}  // namespace

namespace any_vpl {

VplVideoEncoder::VplVideoEncoder(std::shared_ptr<VplSession> session, webrtc::VideoCodecType codec)
    : session_(session), codec_(ToMfxCodec(codec)), bitrateAdjuster_(MIN_ADJUSTED_BITRATE_PERCENTAGE, MAX_ADJUSTED_BITRATE_PERCENTAGE) {}

VplVideoEncoder::~VplVideoEncoder() {
  Release();
}

mfxStatus VplVideoEncoder::ExecQuery(mfxVideoParam& param) {
  mfxVideoParam queryParam;
  memcpy(&queryParam, &param, sizeof(param));
  mfxStatus sts = encoder_->Query(&queryParam, &queryParam);

  if (sts >= 0) {
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
  return sts;
}

mfxStatus VplVideoEncoder::ExecQueries(mfxVideoParam& param, ExtBuffer& ext) {
  mfxStatus sts = MFX_ERR_NONE;

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

  mfxExtBuffer** ext_buffers = ext.ext_buffers;
  mfxExtCodingOption& ext_coding_option = ext.ext_coding_option;
  mfxExtCodingOption2& ext_coding_option2 = ext.ext_coding_option2;
  int ext_buffers_size = 0;

  // In case we need extra configuration, we can uncomment below options
  if (codec_ == MFX_CODEC_AVC) {
    memset(&ext_coding_option, 0, sizeof(ext_coding_option));
    ext_coding_option.Header.BufferId = MFX_EXTBUFF_CODING_OPTION;
    ext_coding_option.Header.BufferSz = sizeof(ext_coding_option);
    ext_coding_option.AUDelimiter = MFX_CODINGOPTION_OFF;
    ext_coding_option.MaxDecFrameBuffering = 1;
    // ext_coding_option.NalHrdConformance = MFX_CODINGOPTION_OFF;
    // ext_coding_option.VuiVclHrdParameters = MFX_CODINGOPTION_ON;
    // ext_coding_option.SingleSeiNalUnit = MFX_CODINGOPTION_ON;
    // ext_coding_option.RefPicMarkRep = MFX_CODINGOPTION_OFF;
    // ext_coding_option.PicTimingSEI = MFX_CODINGOPTION_OFF;
    // ext_coding_option.RecoveryPointSEI = MFX_CODINGOPTION_OFF;
    // ext_coding_option.FramePicture = MFX_CODINGOPTION_OFF;
    // ext_coding_option.FieldOutput = MFX_CODINGOPTION_ON;

    memset(&ext_coding_option2, 0, sizeof(ext_coding_option2));
    ext_coding_option2.Header.BufferId = MFX_EXTBUFF_CODING_OPTION2;
    ext_coding_option2.Header.BufferSz = sizeof(ext_coding_option2);
    ext_coding_option2.RepeatPPS = MFX_CODINGOPTION_ON;
    // ext_coding_option2.MaxSliceSize = 1;
    // ext_coding_option2.AdaptiveI = MFX_CODINGOPTION_ON;

    ext_buffers[0] = (mfxExtBuffer*)&ext_coding_option;
    ext_buffers[1] = (mfxExtBuffer*)&ext_coding_option2;
    ext_buffers_size = 2;
  } else if (codec_ == MFX_CODEC_HEVC) {
    memset(&ext_coding_option2, 0, sizeof(ext_coding_option2));
    ext_coding_option2.Header.BufferId = MFX_EXTBUFF_CODING_OPTION2;
    ext_coding_option2.Header.BufferSz = sizeof(ext_coding_option2);
    ext_coding_option2.RepeatPPS = MFX_CODINGOPTION_ON;

    ext_buffers[0] = (mfxExtBuffer*)&ext_coding_option2;
    ext_buffers_size = 1;
  }

  if (ext_buffers_size != 0) {
    param.ExtParam = ext_buffers;
    param.NumExtParam = ext_buffers_size;
  }

  sts = ExecQuery(param);
  if (sts >= 0) {
    return sts;
  }

  RTC_LOG(LS_WARNING) << "Unsupported encoder codec: codec=" << CodecToString(codec_) << " sts=" << sts
                      << " ... Retry with IOPattern IN_SYSTEM_MEMORY only";
  param.IOPattern = MFX_IOPATTERN_IN_SYSTEM_MEMORY;
  sts = ExecQuery(param);
  if (sts >= 0) {
    return sts;
  }

  RTC_LOG(LS_ERROR) << "Unsupported encoder codec: codec=" << CodecToString(codec_) << " sts=" << sts;

  return sts;
}

int32_t VplVideoEncoder::InitEncode(const webrtc::VideoCodec* codec_settings, int32_t number_of_cores, size_t max_payload_size) {
  RTC_DCHECK(codec_settings);

  int32_t release_ret = Release();
  if (release_ret != WEBRTC_VIDEO_CODEC_OK) {
    return release_ret;
  }

  width_ = codec_settings->width;
  height_ = codec_settings->height;
  targetBitrateBps_ = codec_settings->startBitrate * 1000;
  maxBitrateBps_ = codec_settings->maxBitrate * 1000;
  bitrateAdjuster_.SetTargetBitrateBps(targetBitrateBps_);
  framerate_ = codec_settings->maxFramerate;
  mode_ = codec_settings->mode;

  RTC_LOG(LS_INFO) << "InitEncode " << targetBitrateBps_ << "bit/sec";

  // Initialize encoded image. Default buffer size: size of unencoded data.
  encodedImage_._encodedWidth = 0;
  encodedImage_._encodedHeight = 0;
  encodedImage_.set_size(0);
  encodedImage_.timing_.flags = webrtc::VideoSendTiming::TimingFrameFlags::kInvalid;
  encodedImage_.content_type_ = (codec_settings->mode == webrtc::VideoCodecMode::kScreensharing) ? webrtc::VideoContentType::SCREENSHARE
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

  // 使ってない入力サーフェスを取り出す
  auto surface = std::find_if(surfaces_.begin(), surfaces_.end(), [](const mfxFrameSurface1& s) { return !s.Data.Locked; });
  if (surface == surfaces_.end()) {
    RTC_LOG(LS_ERROR) << "Surface not found";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // I420 から NV12 に変換
  rtc::scoped_refptr<const webrtc::I420BufferInterface> frame_buffer = frame.video_frame_buffer()->ToI420();
  libyuv::I420ToNV12(frame_buffer->DataY(), frame_buffer->StrideY(), frame_buffer->DataU(), frame_buffer->StrideU(), frame_buffer->DataV(),
                     frame_buffer->StrideV(), surface->Data.Y, surface->Data.Pitch, surface->Data.U, surface->Data.Pitch,
                     frame_buffer->width(), frame_buffer->height());

  mfxStatus sts;

  mfxEncodeCtrl ctrl;
  memset(&ctrl, 0, sizeof(ctrl));
  // send_key_frame = true;
  if (send_key_frame) {
    ctrl.FrameType = MFX_FRAMETYPE_I | MFX_FRAMETYPE_IDR | MFX_FRAMETYPE_REF;
  } else {
    ctrl.FrameType = MFX_FRAMETYPE_UNKNOWN;
  }

  if (reconfigureNeeded_) {
    auto start_time = std::chrono::system_clock::now();
    RTC_LOG(LS_INFO) << "Start reconfigure: bps=" << (bitrateAdjuster_.GetAdjustedBitrateBps() / 1000) << " framerate=" << framerate_;
    // 今の設定を取得する
    mfxVideoParam param;
    memset(&param, 0, sizeof(param));

    sts = encoder_->GetVideoParam(&param);
    VPL_CHECK_RESULT(sts, MFX_ERR_NONE, sts);

    // ビットレートとフレームレートを変更する。
    // なお、encoder_->Reset() はキューイングしているサーフェスを
    // 全て処理してから呼び出す必要がある。
    // ここでは encoder_->Init() の時に
    //   param.mfx.GopRefDist = 1;
    //   param.AsyncDepth = 1;
    //   ext_coding_option.MaxDecFrameBuffering = 1;
    // を設定して、そもそもキューイングが起きないようにすることで対処している。
    if (param.mfx.RateControlMethod == MFX_RATECONTROL_CQP) {
      // param.mfx.QPI = h264BitstreamParser_.GetLastSliceQp().value_or(30);
    } else {
      param.mfx.TargetKbps = bitrateAdjuster_.GetAdjustedBitrateBps() / 1000;
    }
    param.mfx.FrameInfo.FrameRateExtN = framerate_;
    param.mfx.FrameInfo.FrameRateExtD = 1;

    sts = encoder_->Reset(&param);
    VPL_CHECK_RESULT(sts, MFX_ERR_NONE, sts);

    reconfigureNeeded_ = false;

    auto end_time = std::chrono::system_clock::now();
    RTC_LOG(LS_INFO) << "Finish reconfigure: " << std::chrono::duration_cast<std::chrono::milliseconds>(end_time - start_time).count()
                     << " ms";
  }

  // NV12 をハードウェアエンコード
  mfxSyncPoint syncp;
  sts = encoder_->EncodeFrameAsync(&ctrl, &*surface, &bitstream_, &syncp);
  // allocRequest_.NumFrameSuggested が 1 の場合は MFX_ERR_MORE_DATA
  // は発生しない
  if (sts == MFX_ERR_MORE_DATA) {
    // もっと入力が必要なので出直す
    return WEBRTC_VIDEO_CODEC_OK;
  }
  VPL_CHECK_RESULT(sts, MFX_ERR_NONE, sts);

  RTC_LOG(LS_INFO) << "Before MFXVideoCORE_SyncOperation";

  sts = MFXVideoCORE_SyncOperation(session_->GetSession(), syncp, 300000);
  VPL_CHECK_RESULT(sts, MFX_ERR_NONE, sts);

  // RTC_LOG(LS_ERROR) << "SurfaceSize=" << (surface->Data.U - surface->Data.Y);
  // RTC_LOG(LS_ERROR) << "DataLength=" << bitstream_.DataLength;
  {
    uint8_t* p = bitstream_.Data + bitstream_.DataOffset;
    int size = bitstream_.DataLength;
    bitstream_.DataLength = 0;

    // FILE* fp = fopen("test.mp4", "a+");
    // fwrite(p, 1, size, fp);
    // fclose(fp);

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
    key_frame_interval_ += 1;
    if (bitstream_.FrameType & MFX_FRAMETYPE_I || bitstream_.FrameType & MFX_FRAMETYPE_IDR) {
      encodedImage_._frameType = webrtc::VideoFrameType::kVideoFrameKey;
      RTC_LOG(LS_INFO) << "Key Frame Generated: key_frame_interval=" << key_frame_interval_;
      key_frame_interval_ = 0;
    } else {
      encodedImage_._frameType = webrtc::VideoFrameType::kVideoFrameDelta;
    }

    webrtc::CodecSpecificInfo codec_specific;
    if (codec_ == MFX_CODEC_AVC) {
      codec_specific.codecType = webrtc::kVideoCodecH264;
      codec_specific.codecSpecific.H264.packetization_mode = webrtc::H264PacketizationMode::NonInterleaved;

      h264BitstreamParser_.ParseBitstream(encodedImage_);
      encodedImage_.qp_ = h264BitstreamParser_.GetLastSliceQp().value_or(-1);
    } else if (codec_ == MFX_CODEC_HEVC) {
      RTC_LOG(LS_ERROR) << __FUNCTION__ << "Current version of WebRTC used by Livekit doesn't support h265";
    }

    webrtc::EncodedImageCallback::Result result = callback_->OnEncodedImage(encodedImage_, &codec_specific);
    if (result.error != webrtc::EncodedImageCallback::Result::OK) {
      RTC_LOG(LS_ERROR) << __FUNCTION__ << " OnEncodedImage failed error:" << result.error;
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    bitrateAdjuster_.Update(size);
  }

  return WEBRTC_VIDEO_CODEC_OK;
}
void VplVideoEncoder::SetRates(const RateControlParameters& parameters) {
  if (parameters.framerate_fps < 1.0) {
    RTC_LOG(LS_WARNING) << "Invalid frame rate: " << parameters.framerate_fps;
    return;
  }

  uint32_t new_framerate = (uint32_t)parameters.framerate_fps;
  uint32_t new_bitrate = parameters.bitrate.get_sum_bps();
  RTC_LOG(LS_INFO) << __FUNCTION__ << " framerate_:" << framerate_ << " new_framerate: " << new_framerate
                   << " targetBitrateBps_:" << targetBitrateBps_ << " new_bitrate:" << new_bitrate << " maxBitrateBps_:" << maxBitrateBps_;
  framerate_ = new_framerate;
  targetBitrateBps_ = new_bitrate;
  bitrateAdjuster_.SetTargetBitrateBps(targetBitrateBps_);
  reconfigureNeeded_ = true;
}
webrtc::VideoEncoder::EncoderInfo VplVideoEncoder::GetEncoderInfo() const {
  webrtc::VideoEncoder::EncoderInfo info;
  info.supports_native_handle = true;
  info.implementation_name = "libvpl";
  info.scaling_settings = webrtc::VideoEncoder::ScalingSettings(kLowH264QpThreshold, kHighH264QpThreshold);
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
  ExtBuffer ext;
  mfxStatus sts = ExecQueries(param, ext);
  if (sts < MFX_ERR_NONE) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  if (sts > MFX_ERR_NONE) {
    RTC_LOG(LS_WARNING) << "Supported specified codec but has warning: codec=" << CodecToString(codec_) << " sts=" << sts;
  }

  sts = encoder_->Init(&param);
  if (sts != MFX_ERR_NONE) {
    RTC_LOG(LS_ERROR) << "Failed to Init encoder: sts=" << sts;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  sts = MFX_ERR_NONE;
  memset(&param, 0, sizeof(param));

  // Retrieve video parameters selected by encoder.
  // - BufferSizeInKB parameter is required to set bit stream buffer size
  sts = encoder_->GetVideoParam(&param);
  VPL_CHECK_RESULT(sts, MFX_ERR_NONE, sts);
  RTC_LOG(LS_INFO) << "BufferSizeInKB=" << param.mfx.BufferSizeInKB;

  // Query number of required surfaces for encoder
  memset(&allocRequest_, 0, sizeof(allocRequest_));
  sts = encoder_->QueryIOSurf(&param, &allocRequest_);
  VPL_CHECK_RESULT(sts, MFX_ERR_NONE, sts);

  RTC_LOG(LS_INFO) << "Encoder NumFrameSuggested=" << allocRequest_.NumFrameSuggested;

  frameInfo_ = param.mfx.FrameInfo;

  // Initializing the Output Bitstream
  bitstreamBuffer_.resize(param.mfx.BufferSizeInKB * 1000);

  memset(&bitstream_, 0, sizeof(bitstream_));
  bitstream_.MaxLength = bitstreamBuffer_.size();
  bitstream_.Data = bitstreamBuffer_.data();

  // Create the required number of input surfaces
  {
    int width = (allocRequest_.Info.Width + 31) / 32 * 32;
    int height = (allocRequest_.Info.Height + 31) / 32 * 32;
    // Number of bytes per page
    // NV12 => 12 bits per pixel
    int size = width * height * 12 / 8;
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
