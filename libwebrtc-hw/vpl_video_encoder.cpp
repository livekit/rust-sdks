#include "vpl_video_encoder.h"

#include <chrono>
#include <mutex>

// WebRTC
#include <common_video/h264/h264_bitstream_parser.h>
// #include <common_video/h265/h265_bitstream_parser.h>
#include <common_video/include/bitrate_adjuster.h>
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
// #include "utils.h"

namespace any_vpl {

class VplVideoEncoderImpl : public VplVideoEncoder {
 public:
  VplVideoEncoderImpl(std::shared_ptr<VplSession> session, mfxU32 codec);
  ~VplVideoEncoderImpl() override;

  int32_t InitEncode(const webrtc::VideoCodec* codec_settings, int32_t number_of_cores, size_t max_payload_size) override;
  int32_t RegisterEncodeCompleteCallback(webrtc::EncodedImageCallback* callback) override;
  int32_t Release() override;
  int32_t Encode(const webrtc::VideoFrame& frame, const std::vector<webrtc::VideoFrameType>* frame_types) override;
  void SetRates(const RateControlParameters& parameters) override;
  webrtc::VideoEncoder::EncoderInfo GetEncoderInfo() const override;

  static std::unique_ptr<MFXVideoENCODE> CreateEncoder(std::shared_ptr<VplSession> session, mfxU32 codec, int width, int height,
                                                       int framerate, int target_kbps, int max_kbps, bool init);

 private:
  struct ExtBuffer {
    mfxExtBuffer* ext_buffers[10];
    mfxExtCodingOption ext_coding_option;
    mfxExtCodingOption2 ext_coding_option2;
  };
  // いろいろなパターンでクエリを投げて、
  // 成功した時の param を返す
  static mfxStatus Queries(MFXVideoENCODE* encoder, mfxU32 codec, int width, int height, int framerate, int target_kbps, int max_kbps,
                           mfxVideoParam& param, ExtBuffer& ext);

 private:
  std::mutex mutex_;
  webrtc::EncodedImageCallback* callback_ = nullptr;

  uint32_t target_bitrate_bps_ = 0;
  uint32_t max_bitrate_bps_ = 0;
  bool reconfigure_needed_ = false;
  bool use_native_ = false;
  uint32_t width_ = 0;
  uint32_t height_ = 0;
  uint32_t framerate_ = 0;
  webrtc::VideoCodecMode mode_ = webrtc::VideoCodecMode::kRealtimeVideo;
  std::vector<std::vector<uint8_t>> v_packet_;
  webrtc::EncodedImage encoded_image_;
  webrtc::H264BitstreamParser h264_bitstream_parser_;
  // webrtc::H265BitstreamParser h265_bitstream_parser_;

  int32_t InitVpl();
  int32_t ReleaseVpl();

  std::vector<uint8_t> surface_buffer_;
  std::vector<mfxFrameSurface1> surfaces_;

  std::shared_ptr<VplSession> session_;
  mfxU32 codec_;
  webrtc::BitrateAdjuster bitrate_adjuster_;
  mfxFrameAllocRequest alloc_request_;
  std::unique_ptr<MFXVideoENCODE> encoder_;
  std::vector<uint8_t> bitstream_buffer_;
  mfxBitstream bitstream_;
  mfxFrameInfo frame_info_;

  int key_frame_interval_ = 0;
};

const int kLowH264QpThreshold = 34;
const int kHighH264QpThreshold = 40;

VplVideoEncoderImpl::VplVideoEncoderImpl(std::shared_ptr<VplSession> session, mfxU32 codec)
    : session_(session), codec_(codec), bitrate_adjuster_(0.5, 0.95) {}

VplVideoEncoderImpl::~VplVideoEncoderImpl() {
  Release();
}

std::unique_ptr<MFXVideoENCODE> VplVideoEncoderImpl::CreateEncoder(std::shared_ptr<VplSession> session, mfxU32 codec, int width, int height,
                                                                   int framerate, int target_kbps, int max_kbps, bool init) {
  std::unique_ptr<MFXVideoENCODE> encoder(new MFXVideoENCODE(session->GetSession()));

  // mfxPlatform platform;
  // memset(&platform, 0, sizeof(platform));
  // MFXVideoCORE_QueryPlatform(GetVplSession(session), &platform);
  // RTC_LOG(LS_VERBOSE) << "--------------- codec=" << CodecToString(codec)
  //                     << " CodeName=" << platform.CodeName
  //                     << " DeviceId=" << platform.DeviceId
  //                     << " MediaAdapterType=" << platform.MediaAdapterType;

  mfxVideoParam param;
  ExtBuffer ext;
  mfxStatus sts = Queries(encoder.get(), codec, width, height, framerate, target_kbps, max_kbps, param, ext);
  if (sts < MFX_ERR_NONE) {
    return nullptr;
  }
  if (sts > MFX_ERR_NONE) {
    RTC_LOG(LS_VERBOSE) << "Supported specified codec but has warning: codec=" << CodecToString(codec) << " sts=" << sts;
  }

  if (init) {
    sts = encoder->Init(&param);
    if (sts != MFX_ERR_NONE) {
      RTC_LOG(LS_ERROR) << "Failed to Init: sts=" << sts;
      return nullptr;
    }
  }

  return encoder;
}

#define ALIGN16(value) (((value + 15) >> 4) << 4)

// void *InitAcceleratorHandle(mfxSession session, int *fd) {
//     printf("in init accel\n");
//     mfxIMPL impl;
//     mfxStatus sts = MFXQueryIMPL(session, &impl);
//     if (sts != MFX_ERR_NONE)
//         return NULL;

// #ifdef LIBVA_SUPPORT
//     printf("in libva support\n");
//     if ((impl & MFX_IMPL_VIA_VAAPI) == MFX_IMPL_VIA_VAAPI) {
//         if (!fd)
//             return NULL;
//         VADisplay va_dpy = NULL;
//         // initialize VAAPI context and set session handle (req in Linux)
//         *fd = open("/dev/dri/renderD128", O_RDWR);
//         if (*fd >= 0) {
//             va_dpy = vaGetDisplayDRM(*fd);
//             if (va_dpy) {
//                 int major_version = 0, minor_version = 0;
//                 if (VA_STATUS_SUCCESS == vaInitialize(va_dpy, &major_version, &minor_version)) {
//                     MFXVideoCORE_SetHandle(session,
//                                            static_cast<mfxHandleType>(MFX_HANDLE_VA_DISPLAY),
//                                            va_dpy);
//                 }
//             }
//         }
//         return va_dpy;
//     }
// #endif

//     return NULL;
// }

void* InitAcceleratorHandle(mfxSession session, int* fd) {
  printf("in init accel\n");
  mfxIMPL impl;
  mfxStatus sts = MFXQueryIMPL(session, &impl);
  if (sts != MFX_ERR_NONE) return NULL;

  // #ifdef LIBVA_SUPPORT
  printf("in libva support\n");
  if ((impl & MFX_IMPL_VIA_VAAPI) == MFX_IMPL_VIA_VAAPI) {
    if (!fd) return NULL;
    VADisplay va_dpy = NULL;
    // initialize VAAPI context and set session handle (req in Linux)
    *fd = open("/dev/dri/renderD128", O_RDWR);
    if (*fd >= 0) {
      va_dpy = vaGetDisplayDRM(*fd);
      if (va_dpy) {
        int major_version = 0, minor_version = 0;
        if (VA_STATUS_SUCCESS == vaInitialize(va_dpy, &major_version, &minor_version)) {
          MFXVideoCORE_SetHandle(session, static_cast<mfxHandleType>(MFX_HANDLE_VA_DISPLAY), va_dpy);
        }
      }
    }
    return va_dpy;
  }
  // #endif

  return NULL;
}

mfxStatus VplVideoEncoderImpl::Queries(MFXVideoENCODE* encoder, mfxU32 codec, int width, int height, int framerate, int target_kbps,
                                       int max_kbps, mfxVideoParam& param, ExtBuffer& ext) {
  mfxStatus sts = MFX_ERR_NONE;

  memset(&param, 0, sizeof(param));

  //       // Initialize session
  //     mfxLoader loader = MFXLoad();
  //     // VERIFY(NULL != loader, "MFXLoad failed -- is implementation in path?");
  // mfxConfig cfg[2];
  // mfxVariant cfgVal[2];
  //     mfxSession session              = NULL;

  //     // Implementation used must be the type requested from command line
  //     cfg[0] = MFXCreateConfig(loader);
  //     // VERIFY(NULL != cfg[0], "MFXCreateConfig failed")
  //     cfgVal[0].Type     = MFX_VARIANT_TYPE_U32;
  //     cfgVal[0].Data.U32 = MFX_IMPL_TYPE_HARDWARE;

  //     sts = MFXSetConfigFilterProperty(cfg[0], (mfxU8 *)"mfxImplDescription.Impl", cfgVal[0]);
  //     // VERIFY(MFX_ERR_NONE == sts, "MFXSetConfigFilterProperty failed for Impl");

  //     // cfg[1] = MFXCreateConfig(loader);
  //     // // VERIFY(NULL != cfg[1], "MFXCreateConfig failed")
  //     // cfgVal[1].Type     = MFX_VARIANT_TYPE_U32;
  //     // cfgVal[1].Data.U32 = MFX_CODEC_AVC;
  //     // sts                = MFXSetConfigFilterProperty(
  //     //     cfg[1],
  //     //     (mfxU8 *)"mfxImplDescription.mfxEncoderDescription.encoder.CodecID",
  //     //     cfgVal[1]);
  //     // VERIFY(MFX_ERR_NONE == sts, "MFXSetConfigFilterProperty failed for encoder CodecID");

  //     sts = MFXCreateSession(loader, 0, &session);
  //     // VERIFY(MFX_ERR_NONE == sts,
  //     //        "Cannot create session -- no implementations meet selection criteria");
  // int accel_fd = 0;
  //            InitAcceleratorHandle(session, &accel_fd);

  param.mfx.CodecId = codec;
  if (codec == MFX_CODEC_VP8) {
    // param.mfx.CodecProfile = MFX_PROFILE_VP8_0;
  } else if (codec == MFX_CODEC_VP9) {
    // param.mfx.CodecProfile = MFX_PROFILE_VP9_0;
  } else if (codec == MFX_CODEC_AVC) {
    // param.mfx.CodecProfile = MFX_PROFILE_AVC_HIGH;
    // param.mfx.CodecLevel = MFX_LEVEL_AVC_51;
    // param.mfx.CodecProfile = MFX_PROFILE_AVC_MAIN;
    // param.mfx.CodecLevel = MFX_LEVEL_AVC_1;
  } else if (codec == MFX_CODEC_HEVC) {
    // param.mfx.CodecProfile = MFX_PROFILE_HEVC_MAIN;
    // param.mfx.CodecLevel = MFX_LEVEL_HEVC_1;
    // param.mfx.LowPower = MFX_CODINGOPTION_OFF;
  } else if (codec == MFX_CODEC_AV1) {
    // param.mfx.CodecProfile = MFX_PROFILE_AV1_MAIN;
  }

  param.mfx.TargetUsage = MFX_TARGETUSAGE_BALANCED;

  // param.mfx.TargetKbps = 4000;
  // //param.mfx.MaxKbps = max_kbps;
  // param.mfx.RateControlMethod = MFX_RATECONTROL_VBR;
  // param.mfx.FrameInfo.FrameRateExtN = 30;
  // param.mfx.FrameInfo.FrameRateExtD = 1;
  // param.mfx.FrameInfo.FourCC = MFX_FOURCC_NV12;
  // param.mfx.FrameInfo.ChromaFormat = MFX_CHROMAFORMAT_YUV420;
  // param.mfx.FrameInfo.PicStruct = MFX_PICSTRUCT_PROGRESSIVE;
  // param.mfx.FrameInfo.CropX = 0;
  // param.mfx.FrameInfo.CropY = 0;
  // param.mfx.FrameInfo.CropW = 1280;
  // param.mfx.FrameInfo.CropH = 720;
  // // Width must be a multiple of 16
  // // Height must be a multiple of 16 in case of frame picture and a multiple of
  // // 32 in case of field picture
  // param.mfx.FrameInfo.Width = ALIGN16(1280);
  // param.mfx.FrameInfo.Height = ALIGN16(720);

  param.mfx.TargetKbps = target_kbps;
  param.mfx.MaxKbps = max_kbps;
  param.mfx.RateControlMethod = MFX_RATECONTROL_VBR;
  param.mfx.FrameInfo.FrameRateExtN = framerate;
  param.mfx.FrameInfo.FrameRateExtD = 1;
  param.mfx.FrameInfo.FourCC = MFX_FOURCC_NV12;
  param.mfx.FrameInfo.ChromaFormat = MFX_CHROMAFORMAT_YUV420;
  param.mfx.FrameInfo.PicStruct = MFX_PICSTRUCT_PROGRESSIVE;
  param.mfx.FrameInfo.CropX = 0;
  param.mfx.FrameInfo.CropY = 0;
  param.mfx.FrameInfo.CropW = width;
  param.mfx.FrameInfo.CropH = height;
  // Width must be a multiple of 16
  // Height must be a multiple of 16 in case of frame picture and a multiple of
  // 32 in case of field picture
  param.mfx.FrameInfo.Width = ALIGN16(width);
  param.mfx.FrameInfo.Height = ALIGN16(height);

  // // sts = encoder->Query(&param, &param);
  // sts = MFXVideoENCODE_Query(session, &param, &param);

  //   // sts = MFXVideoENCODE_Query(session, &param, &param);
  //   RTC_LOG(LS_INFO) << "===== encode query sts:" << sts;

  param.mfx.GopRefDist = 1;
  param.AsyncDepth = 1;
  // param.IOPattern =
  //    MFX_IOPATTERN_IN_SYSTEM_MEMORY | MFX_IOPATTERN_OUT_SYSTEM_MEMORY;
  param.IOPattern = MFX_IOPATTERN_IN_SYSTEM_MEMORY;
  mfxExtBuffer** ext_buffers = ext.ext_buffers;
  mfxExtCodingOption& ext_coding_option = ext.ext_coding_option;
  mfxExtCodingOption2& ext_coding_option2 = ext.ext_coding_option2;
  int ext_buffers_size = 0;
  if (codec == MFX_CODEC_AVC) {
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
  } else if (codec == MFX_CODEC_HEVC) {
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

  // Query 関数を呼び出す。
  // 失敗した場合 param は一切書き換わらない
  // 成功した場合 param は書き換わる可能性がある
  auto query = [](MFXVideoENCODE* encoder, mfxVideoParam& param) {
    mfxVideoParam query_param;
    memcpy(&query_param, &param, sizeof(param));
    // ドキュメントによると、Query は以下のエラーを返す可能性がある。
    // MFX_ERR_NONE	The function completed successfully.
    // MFX_ERR_UNSUPPORTED	The function failed to identify a specific
    // implementation for the required features. MFX_WRN_PARTIAL_ACCELERATION
    // The underlying hardware does not fully support the specified video
    // parameters; The encoding may be partially accelerated. Only SDK HW
    // implementations may return this status code.
    // MFX_WRN_INCOMPATIBLE_VIDEO_PARAM	The function detected some video
    // parameters were incompatible with others; incompatibility resolved.
    mfxStatus sts = encoder->Query(&query_param, &query_param);
    if (sts >= 0) {
// デバッグ用。
// Query によってどのパラメータが変更されたかを表示する
#define F(NAME) \
  if (param.NAME != query_param.NAME) RTC_LOG(LS_VERBOSE) << "param " << #NAME << " old=" << param.NAME << " new=" << query_param.NAME
      F(mfx.LowPower);
      F(mfx.BRCParamMultiplier);
      F(mfx.FrameInfo.FrameRateExtN);
      F(mfx.FrameInfo.FrameRateExtD);
      F(mfx.FrameInfo.FourCC);
      F(mfx.FrameInfo.ChromaFormat);
      F(mfx.FrameInfo.PicStruct);
      F(mfx.FrameInfo.CropX);
      F(mfx.FrameInfo.CropY);
      F(mfx.FrameInfo.CropW);
      F(mfx.FrameInfo.CropH);
      F(mfx.FrameInfo.Width);
      F(mfx.FrameInfo.Height);
      F(mfx.CodecId);
      F(mfx.CodecProfile);
      F(mfx.CodecLevel);
      F(mfx.GopPicSize);
      F(mfx.GopRefDist);
      F(mfx.GopOptFlag);
      F(mfx.IdrInterval);
      F(mfx.TargetUsage);
      F(mfx.RateControlMethod);
      F(mfx.InitialDelayInKB);
      F(mfx.TargetKbps);
      F(mfx.MaxKbps);
      F(mfx.BufferSizeInKB);
      F(mfx.NumSlice);
      F(mfx.NumRefFrame);
      F(mfx.EncodedOrder);
      F(mfx.DecodedOrder);
      F(mfx.ExtendedPicStruct);
      F(mfx.TimeStampCalc);
      F(mfx.SliceGroupsPresent);
      F(mfx.MaxDecFrameBuffering);
      F(mfx.EnableReallocRequest);
      F(AsyncDepth);
      F(IOPattern);
#undef F

      memcpy(&param, &query_param, sizeof(param));
    }
    return sts;
  };

  // ここからは、ひたすらパラメータを変えて query を呼び出していく
  sts = query(encoder, param);
  if (sts >= 0) {
    return sts;
  }

  // IOPattern を MFX_IOPATTERN_IN_SYSTEM_MEMORY のみにしてみる
  // Coffee Lake の H265 はこのパターンでないと通らない
  RTC_LOG(LS_VERBOSE) << "Unsupported encoder codec: codec=" << CodecToString(codec) << " sts=" << sts
                      << " ... Retry with IOPattern IN_SYSTEM_MEMORY only";
  param.IOPattern = MFX_IOPATTERN_IN_SYSTEM_MEMORY;
  sts = query(encoder, param);
  if (sts >= 0) {
    return sts;
  }

  // LowPower ON にして、更に H264/H265 は固定 QP モードにしてみる
  RTC_LOG(LS_VERBOSE) << "Unsupported encoder codec: codec=" << CodecToString(codec) << " sts=" << sts << " ... Retry with low power mode";
  param.mfx.LowPower = MFX_CODINGOPTION_ON;
  if (codec == MFX_CODEC_AVC || codec == MFX_CODEC_HEVC) {
    param.mfx.RateControlMethod = MFX_RATECONTROL_CQP;
    param.mfx.QPI = 25;
    param.mfx.QPP = 33;
    param.mfx.QPB = 40;
  }
  sts = query(encoder, param);
  if (sts >= 0) {
    return sts;
  }
  RTC_LOG(LS_VERBOSE) << "Unsupported encoder codec: codec=" << CodecToString(codec) << " sts=" << sts;

  return sts;
}

int32_t VplVideoEncoderImpl::InitEncode(const webrtc::VideoCodec* codec_settings, int32_t number_of_cores, size_t max_payload_size) {
  RTC_DCHECK(codec_settings);

  int32_t release_ret = Release();
  if (release_ret != WEBRTC_VIDEO_CODEC_OK) {
    return release_ret;
  }

  width_ = codec_settings->width;
  height_ = codec_settings->height;
  target_bitrate_bps_ = codec_settings->startBitrate * 1000;
  max_bitrate_bps_ = codec_settings->maxBitrate * 1000;
  bitrate_adjuster_.SetTargetBitrateBps(target_bitrate_bps_);
  framerate_ = codec_settings->maxFramerate;
  mode_ = codec_settings->mode;

  RTC_LOG(LS_INFO) << "InitEncode " << target_bitrate_bps_ << "bit/sec";

  // Initialize encoded image. Default buffer size: size of unencoded data.
  encoded_image_._encodedWidth = 0;
  encoded_image_._encodedHeight = 0;
  encoded_image_.set_size(0);
  encoded_image_.timing_.flags = webrtc::VideoSendTiming::TimingFrameFlags::kInvalid;
  encoded_image_.content_type_ = (codec_settings->mode == webrtc::VideoCodecMode::kScreensharing) ? webrtc::VideoContentType::SCREENSHARE
                                                                                                  : webrtc::VideoContentType::UNSPECIFIED;

  return InitVpl();
}
int32_t VplVideoEncoderImpl::RegisterEncodeCompleteCallback(webrtc::EncodedImageCallback* callback) {
  std::lock_guard<std::mutex> lock(mutex_);
  callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}
int32_t VplVideoEncoderImpl::Release() {
  return ReleaseVpl();
}

// void ConvertRGBToNV12(const webrtc::VideoFrame &frame, mfxFrameSurface1 *surface) {
//     mfxU16 w = surface->Info.CropW;
//     mfxU16 h = surface->Info.CropH;
//     mfxU8 *y_ptr = surface->Data.Y;
//     mfxU8 *uv_ptr = surface->Data.UV;
//     int pitch = surface->Data.Pitch;

//     const uint8_t *rgb_data = frame.video_frame_buffer()->DataRGB();
//     int rgb_stride = frame.video_frame_buffer()->StrideRGB();

//     for (mfxU16 i = 0; i < h; i++) {
//         for (mfxU16 j = 0; j < w; j++) {
//             int r = rgb_data[i * rgb_stride + j * 3];
//             int g = rgb_data[i * rgb_stride + j * 3 + 1];
//             int b = rgb_data[i * rgb_stride + j * 3 + 2];

//             // Convert RGB to YUV
//             int y = (77 * r + 150 * g + 29 * b) >> 8;
//             int u = ((-43 * r - 85 * g + 128 * b) >> 8) + 128;
//             int v = ((128 * r - 107 * g - 21 * b) >> 8) + 128;

//             y_ptr[i * pitch + j] = static_cast<mfxU8>(y);
//             if (i % 2 == 0 && j % 2 == 0) {
//                 uv_ptr[(i / 2) * pitch + j] = static_cast<mfxU8>(u);
//                 uv_ptr[(i / 2) * pitch + j + 1] = static_cast<mfxU8>(v);
//             }
//         }
//     }
// }

// mfxStatus MyReadRawFrame(mfxFrameSurface1 *surface, const webrtc::VideoFrame &frame) {
//     mfxU16 w, h, i, pitch;
//     const uint8_t *ptr;
//     mfxFrameInfo *info = &surface->Info;
//     mfxFrameData *data = &surface->Data;

//     w = info->CropW;
//     h = info->CropH;

//     switch (info->FourCC) {
//         case MFX_FOURCC_I420:
//             // read luminance plane (Y)
//             pitch = data->Pitch;
//             ptr   = frame.video_frame_buffer()->DataY();
//             for (i = 0; i < h; i++) {
//                 memcpy(data->Y + i * pitch, ptr + i * frame.video_frame_buffer()->StrideY(), w);
//             }

//             // read chrominance (U, V)
//             pitch /= 2;
//             h /= 2;
//             w /= 2;
//             ptr = frame.video_frame_buffer()->DataU();
//             for (i = 0; i < h; i++) {
//                 memcpy(data->U + i * pitch, ptr + i * frame.video_frame_buffer()->StrideU(), w);
//             }

//             ptr = frame.video_frame_buffer()->DataV();
//             for (i = 0; i < h; i++) {
//                 memcpy(data->V + i * pitch, ptr + i * frame.video_frame_buffer()->StrideV(), w);
//             }
//             break;
//         case MFX_FOURCC_NV12:
//             if (frame.video_frame_buffer()->Type() == webrtc::VideoFrameBuffer::Type::kRGB) {
//                 // Convert RGB to NV12
//                 ConvertRGBToNV12(frame, surface);
//             } else {
//                 // Y
//                 pitch = data->Pitch;
//                 ptr   = frame.video_frame_buffer()->DataY();
//                 for (i = 0; i < h; i++) {
//                     memcpy(data->Y + i * pitch, ptr + i * frame.video_frame_buffer()->StrideY(), w);
//                 }
//                 // UV
//                 h /= 2;
//                 ptr = frame.video_frame_buffer()->DataUV();
//                 for (i = 0; i < h; i++) {
//                     memcpy(data->UV + i * pitch, ptr + i * frame.video_frame_buffer()->StrideUV(), w);
//                 }
//             }
//             break;
//         case MFX_FOURCC_RGB4:
//             // B
//             pitch = data->Pitch;
//             ptr   = frame.video_frame_buffer()->DataB();
//             for (i = 0; i < h; i++) {
//                 memcpy(data->B + i * pitch, ptr + i * frame.video_frame_buffer()->StrideB(), pitch);
//             }
//             break;
//         default:
//             printf("Unsupported FourCC code, skip LoadRawFrame\n");
//             break;
//     }

//     return MFX_ERR_NONE;
// }

int32_t VplVideoEncoderImpl::Encode(const webrtc::VideoFrame& frame, const std::vector<webrtc::VideoFrameType>* frame_types) {
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

  // sts = ReadRawFrame_InternalMem(surface, source);
  // libyuv::I400Copy(frame.video_frame_buffer()->GetNV12()->DataY(), frame.video_frame_buffer()->GetNV12()->StrideY(), surface->Data.Y,
  // surface->Data.Pitch, frame.width(), frame.height());

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

  if (reconfigure_needed_) {
    auto start_time = std::chrono::system_clock::now();
    RTC_LOG(LS_INFO) << "Start reconfigure: bps=" << (bitrate_adjuster_.GetAdjustedBitrateBps() / 1000) << " framerate=" << framerate_;
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
      // param.mfx.QPI = h264_bitstream_parser_.GetLastSliceQp().value_or(30);
    } else {
      param.mfx.TargetKbps = bitrate_adjuster_.GetAdjustedBitrateBps() / 1000;
    }
    param.mfx.FrameInfo.FrameRateExtN = framerate_;
    param.mfx.FrameInfo.FrameRateExtD = 1;

    sts = encoder_->Reset(&param);
    VPL_CHECK_RESULT(sts, MFX_ERR_NONE, sts);

    reconfigure_needed_ = false;

    auto end_time = std::chrono::system_clock::now();
    RTC_LOG(LS_INFO) << "Finish reconfigure: " << std::chrono::duration_cast<std::chrono::milliseconds>(end_time - start_time).count()
                     << " ms";
  }

  // NV12 をハードウェアエンコード
  mfxSyncPoint syncp;
  sts = encoder_->EncodeFrameAsync(&ctrl, &*surface, &bitstream_, &syncp);
  // alloc_request_.NumFrameSuggested が 1 の場合は MFX_ERR_MORE_DATA
  // は発生しない
  if (sts == MFX_ERR_MORE_DATA) {
    // もっと入力が必要なので出直す
    return WEBRTC_VIDEO_CODEC_OK;
  }
  VPL_CHECK_RESULT(sts, MFX_ERR_NONE, sts);

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
    encoded_image_.SetEncodedData(buf);
    encoded_image_._encodedWidth = width_;
    encoded_image_._encodedHeight = height_;
    encoded_image_.content_type_ =
        (mode_ == webrtc::VideoCodecMode::kScreensharing) ? webrtc::VideoContentType::SCREENSHARE : webrtc::VideoContentType::UNSPECIFIED;
    encoded_image_.timing_.flags = webrtc::VideoSendTiming::kInvalid;
    encoded_image_.SetTimestamp(frame.timestamp());
    encoded_image_.ntp_time_ms_ = frame.ntp_time_ms();
    encoded_image_.capture_time_ms_ = frame.render_time_ms();
    encoded_image_.rotation_ = frame.rotation();
    encoded_image_.SetColorSpace(frame.color_space());
    key_frame_interval_ += 1;
    if (bitstream_.FrameType & MFX_FRAMETYPE_I || bitstream_.FrameType & MFX_FRAMETYPE_IDR) {
      encoded_image_._frameType = webrtc::VideoFrameType::kVideoFrameKey;
      RTC_LOG(LS_INFO) << "Key Frame Generated: key_frame_interval=" << key_frame_interval_;
      key_frame_interval_ = 0;
    } else {
      encoded_image_._frameType = webrtc::VideoFrameType::kVideoFrameDelta;
    }

    webrtc::CodecSpecificInfo codec_specific;
    if (codec_ == MFX_CODEC_AVC) {
      codec_specific.codecType = webrtc::kVideoCodecH264;
      codec_specific.codecSpecific.H264.packetization_mode = webrtc::H264PacketizationMode::NonInterleaved;

      h264_bitstream_parser_.ParseBitstream(encoded_image_);
      encoded_image_.qp_ = h264_bitstream_parser_.GetLastSliceQp().value_or(-1);
    } else if (codec_ == MFX_CODEC_HEVC) {
      RTC_LOG(LS_ERROR) << __FUNCTION__ << "Current version of WebRTC used by Livekit doesn't support h265";
      // codec_specific.codecType = webrtc::kVideoCodecH265;

      // h265_bitstream_parser_.ParseBitstream(encoded_image_);
      // encoded_image_.qp_ =
      // h265_bitstream_parser_.GetLastSliceQp().value_or(-1);
    }

    webrtc::EncodedImageCallback::Result result = callback_->OnEncodedImage(encoded_image_, &codec_specific);
    if (result.error != webrtc::EncodedImageCallback::Result::OK) {
      RTC_LOG(LS_ERROR) << __FUNCTION__ << " OnEncodedImage failed error:" << result.error;
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    bitrate_adjuster_.Update(size);
  }

  return WEBRTC_VIDEO_CODEC_OK;
}
void VplVideoEncoderImpl::SetRates(const RateControlParameters& parameters) {
  if (parameters.framerate_fps < 1.0) {
    RTC_LOG(LS_WARNING) << "Invalid frame rate: " << parameters.framerate_fps;
    return;
  }

  uint32_t new_framerate = (uint32_t)parameters.framerate_fps;
  uint32_t new_bitrate = parameters.bitrate.get_sum_bps();
  RTC_LOG(LS_INFO) << __FUNCTION__ << " framerate_:" << framerate_ << " new_framerate: " << new_framerate
                   << " target_bitrate_bps_:" << target_bitrate_bps_ << " new_bitrate:" << new_bitrate
                   << " max_bitrate_bps_:" << max_bitrate_bps_;
  framerate_ = new_framerate;
  target_bitrate_bps_ = new_bitrate;
  bitrate_adjuster_.SetTargetBitrateBps(target_bitrate_bps_);
  reconfigure_needed_ = true;
}
webrtc::VideoEncoder::EncoderInfo VplVideoEncoderImpl::GetEncoderInfo() const {
  webrtc::VideoEncoder::EncoderInfo info;
  info.supports_native_handle = true;
  info.implementation_name = "libvpl";
  info.scaling_settings = webrtc::VideoEncoder::ScalingSettings(kLowH264QpThreshold, kHighH264QpThreshold);
  info.is_hardware_accelerated = true;
  return info;
}

int32_t VplVideoEncoderImpl::InitVpl() {
  encoder_ = CreateEncoder(session_, codec_, width_, height_, framerate_, bitrate_adjuster_.GetAdjustedBitrateBps() / 1000,
                           max_bitrate_bps_ / 1000, true);
  if (encoder_ == nullptr) {
    RTC_LOG(LS_ERROR) << "Failed to create encoder";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  mfxStatus sts = MFX_ERR_NONE;

  mfxVideoParam param;
  memset(&param, 0, sizeof(param));

  // Retrieve video parameters selected by encoder.
  // - BufferSizeInKB parameter is required to set bit stream buffer size
  sts = encoder_->GetVideoParam(&param);
  VPL_CHECK_RESULT(sts, MFX_ERR_NONE, sts);
  RTC_LOG(LS_INFO) << "BufferSizeInKB=" << param.mfx.BufferSizeInKB;

  // Query number of required surfaces for encoder
  memset(&alloc_request_, 0, sizeof(alloc_request_));
  sts = encoder_->QueryIOSurf(&param, &alloc_request_);
  VPL_CHECK_RESULT(sts, MFX_ERR_NONE, sts);

  RTC_LOG(LS_INFO) << "Encoder NumFrameSuggested=" << alloc_request_.NumFrameSuggested;

  frame_info_ = param.mfx.FrameInfo;

  // 出力ビットストリームの初期化
  bitstream_buffer_.resize(param.mfx.BufferSizeInKB * 1000);

  memset(&bitstream_, 0, sizeof(bitstream_));
  bitstream_.MaxLength = bitstream_buffer_.size();
  bitstream_.Data = bitstream_buffer_.data();

  // 必要な枚数分の入力サーフェスを作る
  {
    int width = (alloc_request_.Info.Width + 31) / 32 * 32;
    int height = (alloc_request_.Info.Height + 31) / 32 * 32;
    // 1枚あたりのバイト数
    // NV12 なので 1 ピクセルあたり 12 ビット
    int size = width * height * 12 / 8;
    surface_buffer_.resize(alloc_request_.NumFrameSuggested * size);

    surfaces_.clear();
    surfaces_.reserve(alloc_request_.NumFrameSuggested);
    for (int i = 0; i < alloc_request_.NumFrameSuggested; i++) {
      mfxFrameSurface1 surface;
      memset(&surface, 0, sizeof(surface));
      surface.Info = frame_info_;
      surface.Data.Y = surface_buffer_.data() + i * size;
      surface.Data.U = surface_buffer_.data() + i * size + width * height;
      surface.Data.V = surface_buffer_.data() + i * size + width * height + 1;
      surface.Data.Pitch = width;
      surfaces_.push_back(surface);
    }
  }

  return WEBRTC_VIDEO_CODEC_OK;
}
int32_t VplVideoEncoderImpl::ReleaseVpl() {
  if (encoder_ != nullptr) {
    encoder_->Close();
  }
  encoder_.reset();
  return WEBRTC_VIDEO_CODEC_OK;
}

////////////////////////
// VplVideoEncoder
////////////////////////

bool VplVideoEncoder::IsSupported(std::shared_ptr<VplSession> session, webrtc::VideoCodecType codec) {
  if (session == nullptr) {
    return false;
  }

  // FIXME(melpon): IsSupported(VP9) == true
  // になるにも関わらず、実際に使ってみると
  // 実行時エラーでクラッシュするため、とりあえず VP9
  // だったら未サポートとして返す。 （VPL の問題なのか使い方の問題なのかは不明）
  if (codec == webrtc::kVideoCodecVP9) {
    return false;
  }

  auto encoder = VplVideoEncoderImpl::CreateEncoder(session, ToMfxCodec(codec), 1920, 1080, 30, 10, 20, false);
  bool result = encoder != nullptr;
  RTC_LOG(LS_VERBOSE) << "IsSupported: codec=" << CodecToString(ToMfxCodec(codec)) << " result=" << result;
  return result;
}

std::unique_ptr<VplVideoEncoder> VplVideoEncoder::Create(std::shared_ptr<VplSession> session, webrtc::VideoCodecType codec) {
  return std::unique_ptr<VplVideoEncoder>(new VplVideoEncoderImpl(session, ToMfxCodec(codec)));
}

}  // namespace any_vpl
