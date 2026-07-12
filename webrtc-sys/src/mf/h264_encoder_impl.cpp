/*
 * Copyright 2025 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "h264_encoder_impl.h"

#include <algorithm>
#include <limits>
#include <string>
#include <utility>

#include <common_video/h264/h264_common.h>
#include "common_video/libyuv/include/webrtc_libyuv.h"
#include "mf_common.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "modules/video_coding/utility/simulcast_rate_allocator.h"
#include "rtc_base/checks.h"
#include "rtc_base/logging.h"
#include "system_wrappers/include/metrics.h"
#include "third_party/libyuv/include/libyuv/convert_from.h"

namespace webrtc {

using livekit_ffi::ComPtr;
using livekit_ffi::HResultToString;

namespace {

// Used by histograms. Values of entries should not be changed.
enum H264EncoderImplEvent {
  kH264EncoderEventInit = 0,
  kH264EncoderEventError = 1,
  kH264EncoderEventMax = 16,
};

// Async hardware MFTs post METransformNeedInput almost immediately, but the
// very first request after initialization can lag while the driver spins up.
constexpr int kNeedInputTimeoutMs = 500;
constexpr int kOutputWaitTimeoutMs = 500;
// How many frames may be in flight inside the MFT before Encode() blocks
// waiting for output. Low-latency mode keeps this near 1 in practice.
constexpr size_t kMaxPendingFrames = 4;

HRESULT SetCodecApiUInt32(ICodecAPI* api, const GUID& guid, UINT32 value) {
  VARIANT v = {};
  v.vt = VT_UI4;
  v.ulVal = value;
  return api->SetValue(&guid, &v);
}

HRESULT SetCodecApiBool(ICodecAPI* api, const GUID& guid, bool value) {
  VARIANT v = {};
  v.vt = VT_BOOL;
  v.boolVal = value ? VARIANT_TRUE : VARIANT_FALSE;
  return api->SetValue(&guid, &v);
}

UINT32 H264ProfileToMFProfile(H264Profile profile) {
  switch (profile) {
    // eAVEncH264VProfile_ConstrainedBase is not understood by every vendor
    // MFT; Base with webrtc's own constraints applied is the interoperable
    // choice (matches what other MF-based WebRTC stacks negotiate).
    case H264Profile::kProfileConstrainedBaseline:
    case H264Profile::kProfileBaseline:
      return eAVEncH264VProfile_Base;
    case H264Profile::kProfileMain:
      return eAVEncH264VProfile_Main;
    case H264Profile::kProfileConstrainedHigh:
    case H264Profile::kProfileHigh:
      return eAVEncH264VProfile_High;
    default:
      return eAVEncH264VProfile_Base;
  }
}

UINT32 H264LevelToMFLevel(H264Level level) {
  switch (level) {
    case H264Level::kLevel1_b:
      return eAVEncH264VLevel1_b;
    case H264Level::kLevel1:
      return eAVEncH264VLevel1;
    case H264Level::kLevel1_1:
      return eAVEncH264VLevel1_1;
    case H264Level::kLevel1_2:
      return eAVEncH264VLevel1_2;
    case H264Level::kLevel1_3:
      return eAVEncH264VLevel1_3;
    case H264Level::kLevel2:
      return eAVEncH264VLevel2;
    case H264Level::kLevel2_1:
      return eAVEncH264VLevel2_1;
    case H264Level::kLevel2_2:
      return eAVEncH264VLevel2_2;
    case H264Level::kLevel3:
      return eAVEncH264VLevel3;
    case H264Level::kLevel3_1:
      return eAVEncH264VLevel3_1;
    case H264Level::kLevel3_2:
      return eAVEncH264VLevel3_2;
    case H264Level::kLevel4:
      return eAVEncH264VLevel4;
    case H264Level::kLevel4_1:
      return eAVEncH264VLevel4_1;
    case H264Level::kLevel4_2:
      return eAVEncH264VLevel4_2;
    case H264Level::kLevel5:
      return eAVEncH264VLevel5;
    case H264Level::kLevel5_1:
      return eAVEncH264VLevel5_1;
    case H264Level::kLevel5_2:
      return eAVEncH264VLevel5_2;
  }
  return eAVEncH264VLevel4;
}

}  // namespace

MFH264EncoderImpl::MFH264EncoderImpl(const Environment& env,
                                     const SdpVideoFormat& format)
    : env_(env), format_(format) {
  std::optional<H264ProfileLevelId> profile_level_id =
      ParseSdpForH264ProfileLevelId(format.parameters);
  if (profile_level_id.has_value()) {
    profile_ = profile_level_id->profile;
    level_ = profile_level_id->level;
  }
}

MFH264EncoderImpl::~MFH264EncoderImpl() {
  Release();
}

void MFH264EncoderImpl::ReportInit() {
  if (has_reported_init_)
    return;
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.H264EncoderImpl.Event",
                            kH264EncoderEventInit, kH264EncoderEventMax);
  has_reported_init_ = true;
}

void MFH264EncoderImpl::ReportError() {
  if (has_reported_error_)
    return;
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.H264EncoderImpl.Event",
                            kH264EncoderEventError, kH264EncoderEventMax);
  has_reported_error_ = true;
}

int32_t MFH264EncoderImpl::InitEncode(const VideoCodec* inst,
                                      const VideoEncoder::Settings& settings) {
  if (!inst || inst->codecType != kVideoCodecH264) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->maxFramerate == 0) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->width < 1 || inst->height < 1) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  int32_t release_ret = Release();
  if (release_ret != WEBRTC_VIDEO_CODEC_OK) {
    ReportError();
    return release_ret;
  }

  codec_ = *inst;

  // Code expects simulcastStream resolutions to be correct, make sure they are
  // filled even when there are no simulcast layers.
  if (codec_.numberOfSimulcastStreams == 0) {
    codec_.simulcastStream[0].width = codec_.width;
    codec_.simulcastStream[0].height = codec_.height;
  }

  // Initialize encoded image. Default buffer size: size of unencoded data.
  const size_t new_capacity =
      CalcBufferSize(VideoType::kI420, codec_.width, codec_.height);
  encoded_image_.SetEncodedData(EncodedImageBuffer::Create(new_capacity));
  encoded_image_._encodedWidth = codec_.width;
  encoded_image_._encodedHeight = codec_.height;
  encoded_image_.set_size(0);

  configuration_.sending = false;
  configuration_.frame_dropping_on = codec_.GetFrameDropEnabled();
  configuration_.key_frame_interval = codec_.H264()->keyFrameInterval;
  configuration_.width = codec_.width;
  configuration_.height = codec_.height;
  configuration_.max_frame_rate = codec_.maxFramerate;
  configuration_.target_bps = codec_.startBitrate * 1000;
  configuration_.max_bps = codec_.maxBitrate * 1000;

  if (!livekit_ffi::EnsureComInitialized() || !livekit_ffi::EnsureMFStarted()) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  int32_t ret = CreateTransform();
  if (ret != WEBRTC_VIDEO_CODEC_OK) {
    ReportError();
    return ret;
  }
  ret = ConfigureTransform();
  if (ret != WEBRTC_VIDEO_CODEC_OK) {
    ReportError();
    return ret;
  }

  RTC_LOG(LS_INFO) << "MediaFoundation H264 encoder initialized ("
                   << friendly_name_ << "): " << codec_.width << "x"
                   << codec_.height << " @ " << codec_.maxFramerate
                   << "fps, target_bps=" << configuration_.target_bps;

  SimulcastRateAllocator init_allocator(env_, codec_);
  VideoBitrateAllocation allocation =
      init_allocator.Allocate(VideoBitrateAllocationParameters(
          DataRate::KilobitsPerSec(codec_.startBitrate), codec_.maxFramerate));
  SetRates(RateControlParameters(allocation, codec_.maxFramerate));
  ReportInit();
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MFH264EncoderImpl::CreateTransform() {
  MFT_REGISTER_TYPE_INFO input_info = {MFMediaType_Video, MFVideoFormat_NV12};
  MFT_REGISTER_TYPE_INFO output_info = {MFMediaType_Video, MFVideoFormat_H264};

  IMFActivate** activates = nullptr;
  UINT32 count = 0;
  HRESULT hr = MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER,
                         MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
                         &input_info, &output_info, &activates, &count);
  if (FAILED(hr) || count == 0) {
    RTC_LOG(LS_ERROR) << "No hardware H264 encoder MFT found: "
                      << HResultToString(hr);
    if (activates) {
      CoTaskMemFree(activates);
    }
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  activate_ = activates[0];
  for (UINT32 i = 0; i < count; i++) {
    activates[i]->Release();
  }
  CoTaskMemFree(activates);

  WCHAR* name = nullptr;
  UINT32 name_len = 0;
  if (SUCCEEDED(activate_->GetAllocatedString(MFT_FRIENDLY_NAME_Attribute,
                                              &name, &name_len))) {
    // Log-only string; lossy narrowing is fine.
    friendly_name_.assign(name, name + name_len);
    CoTaskMemFree(name);
  }

  hr = activate_->ActivateObject(IID_PPV_ARGS(&transform_));
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "Failed to activate H264 encoder MFT \""
                      << friendly_name_ << "\": " << HResultToString(hr);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  ComPtr<IMFAttributes> attributes;
  if (SUCCEEDED(transform_->GetAttributes(&attributes)) && attributes) {
    UINT32 is_async = 0;
    attributes->GetUINT32(MF_TRANSFORM_ASYNC, &is_async);
    is_async_ = is_async != 0;
    if (is_async_) {
      hr = attributes->SetUINT32(MF_TRANSFORM_ASYNC_UNLOCK, TRUE);
      if (FAILED(hr)) {
        RTC_LOG(LS_ERROR) << "Failed to unlock async MFT: "
                          << HResultToString(hr);
        return WEBRTC_VIDEO_CODEC_ERROR;
      }
    }
  }

  if (is_async_) {
    hr = transform_.As(&event_generator_);
    if (FAILED(hr)) {
      RTC_LOG(LS_ERROR) << "Async MFT without IMFMediaEventGenerator: "
                        << HResultToString(hr);
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }

  // ICodecAPI is how rate control is configured; refuse encoders without it
  // rather than running with driver-default VBR.
  hr = transform_.As(&codec_api_);
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "H264 encoder MFT does not expose ICodecAPI: "
                      << HResultToString(hr);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MFH264EncoderImpl::ConfigureTransform() {
  DWORD input_ids[1] = {0};
  DWORD output_ids[1] = {0};
  HRESULT hr = transform_->GetStreamIDs(1, input_ids, 1, output_ids);
  if (hr == E_NOTIMPL) {
    input_ids[0] = 0;
    output_ids[0] = 0;
  } else if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "GetStreamIDs failed: " << HResultToString(hr);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  input_stream_id_ = input_ids[0];
  output_stream_id_ = output_ids[0];

  const UINT32 fps =
      std::max(1u, static_cast<UINT32>(configuration_.max_frame_rate + 0.5f));

  // Output type first: encoder MFTs require it before the input type.
  ComPtr<IMFMediaType> output_type;
  hr = MFCreateMediaType(&output_type);
  if (FAILED(hr)) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  output_type->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
  output_type->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_H264);
  output_type->SetUINT32(MF_MT_AVG_BITRATE, configuration_.target_bps);
  MFSetAttributeSize(output_type.Get(), MF_MT_FRAME_SIZE, configuration_.width,
                     configuration_.height);
  MFSetAttributeRatio(output_type.Get(), MF_MT_FRAME_RATE, fps, 1);
  output_type->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
  output_type->SetUINT32(MF_MT_MPEG2_PROFILE, H264ProfileToMFProfile(profile_));
  output_type->SetUINT32(MF_MT_MPEG2_LEVEL, H264LevelToMFLevel(level_));

  hr = transform_->SetOutputType(output_stream_id_, output_type.Get(), 0);
  if (FAILED(hr)) {
    // Some drivers reject an explicit level; let the MFT derive it.
    output_type->DeleteItem(MF_MT_MPEG2_LEVEL);
    hr = transform_->SetOutputType(output_stream_id_, output_type.Get(), 0);
  }
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "SetOutputType failed: " << HResultToString(hr);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // Input type: prefer the MFT's own NV12 type so vendor-specific attributes
  // (strides, apertures) are preserved.
  ComPtr<IMFMediaType> input_type;
  for (DWORD i = 0;; i++) {
    ComPtr<IMFMediaType> candidate;
    hr = transform_->GetInputAvailableType(input_stream_id_, i, &candidate);
    if (FAILED(hr)) {
      break;
    }
    GUID subtype = {};
    candidate->GetGUID(MF_MT_SUBTYPE, &subtype);
    if (subtype == MFVideoFormat_NV12) {
      input_type = candidate;
      break;
    }
  }
  if (!input_type) {
    hr = MFCreateMediaType(&input_type);
    if (FAILED(hr)) {
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    input_type->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
    input_type->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_NV12);
  }
  MFSetAttributeSize(input_type.Get(), MF_MT_FRAME_SIZE, configuration_.width,
                     configuration_.height);
  MFSetAttributeRatio(input_type.Get(), MF_MT_FRAME_RATE, fps, 1);
  input_type->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);

  hr = transform_->SetInputType(input_stream_id_, input_type.Get(), 0);
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "SetInputType failed: " << HResultToString(hr);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // Rate control: CBR at the target bitrate, low latency, no B-frames (webrtc
  // cannot tolerate them), and an effectively infinite GOP since webrtc
  // requests IDR frames itself.
  hr = SetCodecApiUInt32(codec_api_.Get(), CODECAPI_AVEncCommonRateControlMode,
                         eAVEncCommonRateControlMode_CBR);
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "Failed to set CBR rate control: "
                      << HResultToString(hr);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  hr = SetCodecApiUInt32(codec_api_.Get(), CODECAPI_AVEncCommonMeanBitRate,
                         configuration_.target_bps);
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "Failed to set mean bitrate: " << HResultToString(hr);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  hr = SetCodecApiUInt32(codec_api_.Get(),
                         CODECAPI_AVEncMPVDefaultBPictureCount, 0);
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "Failed to disable B-frames: " << HResultToString(hr);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  // Best effort from here on: support varies by vendor/driver.
  if (FAILED(SetCodecApiBool(codec_api_.Get(), CODECAPI_AVLowLatencyMode,
                             true))) {
    RTC_LOG(LS_WARNING) << "Encoder MFT rejected AVLowLatencyMode.";
  }
  if (FAILED(SetCodecApiUInt32(codec_api_.Get(), CODECAPI_AVEncMPVGOPSize,
                               0x7FFFFFFF))) {
    RTC_LOG(LS_WARNING) << "Encoder MFT rejected infinite GOP size.";
  }

  active_bitrate_bps_ = configuration_.target_bps;
  dynamic_bitrate_supported_ = true;
  pending_bitrate_reinit_ = false;

  hr = transform_->ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "NOTIFY_BEGIN_STREAMING failed: "
                      << HResultToString(hr);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  hr = transform_->ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0);
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "NOTIFY_START_OF_STREAM failed: "
                      << HResultToString(hr);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  CacheSequenceHeader();
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MFH264EncoderImpl::ReinitTransform() {
  RTC_LOG(LS_INFO) << "Reinitializing MF H264 encoder (bitrate "
                   << active_bitrate_bps_ << " -> "
                   << configuration_.target_bps << " bps).";
  EncodedImageCallback* callback = encoded_image_callback_;
  int32_t ret = Release();
  if (ret != WEBRTC_VIDEO_CODEC_OK) {
    return ret;
  }
  encoded_image_callback_ = callback;
  ret = CreateTransform();
  if (ret != WEBRTC_VIDEO_CODEC_OK) {
    return ret;
  }
  ret = ConfigureTransform();
  if (ret != WEBRTC_VIDEO_CODEC_OK) {
    return ret;
  }
  configuration_.key_frame_request = true;
  return WEBRTC_VIDEO_CODEC_OK;
}

HRESULT MFH264EncoderImpl::NegotiateOutputType() {
  for (DWORD i = 0;; i++) {
    ComPtr<IMFMediaType> candidate;
    HRESULT hr =
        transform_->GetOutputAvailableType(output_stream_id_, i, &candidate);
    if (FAILED(hr)) {
      return hr;
    }
    GUID subtype = {};
    candidate->GetGUID(MF_MT_SUBTYPE, &subtype);
    if (subtype != MFVideoFormat_H264) {
      continue;
    }
    hr = transform_->SetOutputType(output_stream_id_, candidate.Get(), 0);
    if (SUCCEEDED(hr)) {
      sequence_header_.clear();
      CacheSequenceHeader();
    }
    return hr;
  }
}

void MFH264EncoderImpl::CacheSequenceHeader() {
  ComPtr<IMFMediaType> current;
  if (FAILED(transform_->GetOutputCurrentType(output_stream_id_, &current))) {
    return;
  }
  UINT8* blob = nullptr;
  UINT32 blob_size = 0;
  if (SUCCEEDED(current->GetAllocatedBlob(MF_MT_MPEG_SEQUENCE_HEADER, &blob,
                                          &blob_size)) &&
      blob_size > 0) {
    sequence_header_.assign(blob, blob + blob_size);
  }
  if (blob) {
    CoTaskMemFree(blob);
  }
}

int32_t MFH264EncoderImpl::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
  encoded_image_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MFH264EncoderImpl::Release() {
  if (transform_) {
    transform_->ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM,
                               input_stream_id_);
    transform_->ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
    transform_->ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
  }
  event_generator_.Reset();
  codec_api_.Reset();
  transform_.Reset();
  if (activate_) {
    activate_->ShutdownObject();
    activate_.Reset();
  }
  pending_frames_.clear();
  need_input_credits_ = 0;
  frame_count_ = 0;
  sequence_header_.clear();
  is_async_ = false;
  return WEBRTC_VIDEO_CODEC_OK;
}

HRESULT MFH264EncoderImpl::CreateInputSample(const I420BufferInterface& buffer,
                                             int64_t sample_time_100ns,
                                             int64_t duration_100ns,
                                             IMFSample** sample_out) {
  const int width = buffer.width();
  const int height = buffer.height();

  ComPtr<IMFMediaBuffer> media_buffer;
  HRESULT hr = MFCreate2DMediaBuffer(width, height, MFVideoFormat_NV12.Data1,
                                     FALSE, &media_buffer);
  if (FAILED(hr)) {
    return hr;
  }

  ComPtr<IMF2DBuffer> buffer_2d;
  hr = media_buffer.As(&buffer_2d);
  if (FAILED(hr)) {
    return hr;
  }

  BYTE* scanline0 = nullptr;
  LONG pitch = 0;
  hr = buffer_2d->Lock2D(&scanline0, &pitch);
  if (FAILED(hr)) {
    return hr;
  }
  // NV12 is never bottom-up: Y plane at scanline0, interleaved UV plane
  // directly after it.
  uint8_t* dst_y = scanline0;
  uint8_t* dst_uv = scanline0 + static_cast<size_t>(pitch) * height;
  int ret = libyuv::I420ToNV12(buffer.DataY(), buffer.StrideY(), buffer.DataU(),
                               buffer.StrideU(), buffer.DataV(),
                               buffer.StrideV(), dst_y, pitch, dst_uv, pitch,
                               width, height);
  buffer_2d->Unlock2D();
  if (ret != 0) {
    return E_FAIL;
  }

  DWORD contiguous_length = 0;
  if (SUCCEEDED(buffer_2d->GetContiguousLength(&contiguous_length))) {
    media_buffer->SetCurrentLength(contiguous_length);
  }

  ComPtr<IMFSample> sample;
  hr = MFCreateSample(&sample);
  if (FAILED(hr)) {
    return hr;
  }
  sample->AddBuffer(media_buffer.Get());
  sample->SetSampleTime(sample_time_100ns);
  sample->SetSampleDuration(duration_100ns);

  *sample_out = sample.Detach();
  return S_OK;
}

int32_t MFH264EncoderImpl::PumpEvents(int timeout_ms,
                                      bool until_need_input,
                                      size_t until_pending_at_most) {
  const ULONGLONG deadline = GetTickCount64() + timeout_ms;
  for (;;) {
    const bool goals_met = (!until_need_input || need_input_credits_ > 0) &&
                           pending_frames_.size() <= until_pending_at_most;

    ComPtr<IMFMediaEvent> event;
    HRESULT hr = event_generator_->GetEvent(MF_EVENT_FLAG_NO_WAIT, &event);
    if (hr == MF_E_NO_EVENTS_AVAILABLE) {
      if (goals_met) {
        return WEBRTC_VIDEO_CODEC_OK;
      }
      if (GetTickCount64() >= deadline) {
        RTC_LOG(LS_ERROR) << "Timed out waiting for encoder MFT ("
                          << (until_need_input ? "input slot" : "output")
                          << ", " << pending_frames_.size()
                          << " frames in flight).";
        ReportError();
        return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
      }
      Sleep(1);
      continue;
    }
    if (FAILED(hr)) {
      RTC_LOG(LS_ERROR) << "GetEvent failed: " << HResultToString(hr);
      ReportError();
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }

    MediaEventType type = MEUnknown;
    event->GetType(&type);
    if (type == METransformNeedInput) {
      need_input_credits_++;
    } else if (type == METransformHaveOutput) {
      int32_t ret = CollectOneOutput();
      if (ret != WEBRTC_VIDEO_CODEC_OK) {
        return ret;
      }
    }
    // Other events (drain complete, markers) need no handling here.
  }
}

int32_t MFH264EncoderImpl::Encode(
    const VideoFrame& input_frame,
    const std::vector<VideoFrameType>* frame_types) {
  if (!transform_) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!encoded_image_callback_) {
    RTC_LOG(LS_WARNING)
        << "InitEncode() has been called, but a callback function "
           "has not been set with RegisterEncodeCompleteCallback()";
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }

  if (pending_bitrate_reinit_) {
    int32_t ret = ReinitTransform();
    if (ret != WEBRTC_VIDEO_CODEC_OK) {
      ReportError();
      return ret;
    }
  }

  webrtc::scoped_refptr<I420BufferInterface> frame_buffer =
      input_frame.video_frame_buffer()->ToI420();
  if (!frame_buffer) {
    RTC_LOG(LS_ERROR) << "Failed to convert "
                      << VideoFrameBufferTypeToString(
                             input_frame.video_frame_buffer()->type())
                      << " image to I420. Can't encode frame.";
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }
  RTC_CHECK(frame_buffer->type() == VideoFrameBuffer::Type::kI420);

  bool is_keyframe_needed = false;
  if (configuration_.key_frame_request && configuration_.sending) {
    is_keyframe_needed = true;
  }

  bool send_key_frame =
      is_keyframe_needed ||
      (frame_types && (*frame_types)[0] == VideoFrameType::kVideoFrameKey);
  if (send_key_frame) {
    is_keyframe_needed = true;
    configuration_.key_frame_request = false;
  }

  RTC_DCHECK_EQ(configuration_.width, frame_buffer->width());
  RTC_DCHECK_EQ(configuration_.height, frame_buffer->height());

  if (!configuration_.sending) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  if (frame_types != nullptr) {
    // Skip frame?
    if ((*frame_types)[0] == VideoFrameType::kEmptyFrame) {
      return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
    }
  }

  if (is_async_) {
    int32_t ret = PumpEvents(kNeedInputTimeoutMs, /*until_need_input=*/true,
                             /*until_pending_at_most=*/SIZE_MAX);
    if (ret != WEBRTC_VIDEO_CODEC_OK) {
      return ret;
    }
  }

  if (is_keyframe_needed) {
    HRESULT hr = SetCodecApiUInt32(codec_api_.Get(),
                                   CODECAPI_AVEncVideoForceKeyFrame, 1);
    if (FAILED(hr)) {
      RTC_LOG(LS_WARNING) << "ForceKeyFrame rejected: " << HResultToString(hr);
    }
  }

  const int64_t fps =
      std::max<int64_t>(1, static_cast<int64_t>(configuration_.max_frame_rate));
  const int64_t duration_100ns = 10'000'000 / fps;
  const int64_t sample_time_100ns = frame_count_ * duration_100ns;
  frame_count_++;

  ComPtr<IMFSample> sample;
  HRESULT hr = CreateInputSample(*frame_buffer, sample_time_100ns,
                                 duration_100ns, &sample);
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "Failed to create input sample: "
                      << HResultToString(hr);
    ReportError();
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  PendingFrameInfo info;
  info.sample_time_100ns = sample_time_100ns;
  info.rtp_timestamp = input_frame.rtp_timestamp();
  info.ntp_time_ms = input_frame.ntp_time_ms();
  info.render_time_ms = input_frame.render_time_ms();
  info.rotation = input_frame.rotation();
  info.color_space = input_frame.color_space();
  pending_frames_.push_back(info);

  hr = transform_->ProcessInput(input_stream_id_, sample.Get(), 0);
  if (hr == MF_E_NOTACCEPTING && !is_async_) {
    int32_t ret = CollectOutputsSync();
    if (ret != WEBRTC_VIDEO_CODEC_OK) {
      return ret;
    }
    hr = transform_->ProcessInput(input_stream_id_, sample.Get(), 0);
  }
  if (FAILED(hr)) {
    pending_frames_.pop_back();
    RTC_LOG(LS_ERROR) << "ProcessInput failed: " << HResultToString(hr);
    ReportError();
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  if (is_async_) {
    need_input_credits_--;
    // Collect whatever the MFT already produced; block only when too many
    // frames are in flight so encoder latency stays bounded.
    return PumpEvents(kOutputWaitTimeoutMs, /*until_need_input=*/false,
                      /*until_pending_at_most=*/kMaxPendingFrames);
  }
  return CollectOutputsSync();
}

int32_t MFH264EncoderImpl::CollectOutputsSync() {
  for (;;) {
    int32_t ret = CollectOneOutput();
    if (ret == WEBRTC_VIDEO_CODEC_NO_OUTPUT) {
      return WEBRTC_VIDEO_CODEC_OK;
    }
    if (ret != WEBRTC_VIDEO_CODEC_OK) {
      return ret;
    }
  }
}

int32_t MFH264EncoderImpl::CollectOneOutput() {
  MFT_OUTPUT_STREAM_INFO stream_info = {};
  HRESULT hr = transform_->GetOutputStreamInfo(output_stream_id_, &stream_info);
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "GetOutputStreamInfo failed: " << HResultToString(hr);
    ReportError();
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }
  const bool transform_allocates =
      stream_info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES |
                             MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES);

  ComPtr<IMFSample> allocated;
  if (!transform_allocates) {
    hr = MFCreateSample(&allocated);
    if (FAILED(hr)) {
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }
    ComPtr<IMFMediaBuffer> out_buffer;
    const DWORD size = stream_info.cbSize
                           ? stream_info.cbSize
                           : static_cast<DWORD>(configuration_.width) *
                                 configuration_.height * 3 / 2;
    hr = MFCreateAlignedMemoryBuffer(
        size, stream_info.cbAlignment > 1 ? stream_info.cbAlignment - 1 : 0,
        &out_buffer);
    if (FAILED(hr)) {
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }
    allocated->AddBuffer(out_buffer.Get());
  }

  for (int attempt = 0; attempt < 2; attempt++) {
    MFT_OUTPUT_DATA_BUFFER output = {};
    output.dwStreamID = output_stream_id_;
    output.pSample = transform_allocates ? nullptr : allocated.Get();
    DWORD status = 0;
    hr = transform_->ProcessOutput(0, 1, &output, &status);
    if (output.pEvents) {
      output.pEvents->Release();
    }

    if (hr == MF_E_TRANSFORM_NEED_MORE_INPUT) {
      return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
    }
    if (hr == MF_E_TRANSFORM_STREAM_CHANGE) {
      HRESULT nhr = NegotiateOutputType();
      if (FAILED(nhr)) {
        RTC_LOG(LS_ERROR) << "Output renegotiation failed: "
                          << HResultToString(nhr);
        ReportError();
        return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
      }
      // Async MFTs re-signal METransformHaveOutput after a stream change; an
      // immediate ProcessOutput retry returns E_UNEXPECTED (seen on Intel
      // QSV). Only sync MFTs retry inline.
      if (is_async_) {
        return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
      }
      continue;
    }
    if (FAILED(hr)) {
      RTC_LOG(LS_ERROR) << "ProcessOutput failed: " << HResultToString(hr);
      ReportError();
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }

    ComPtr<IMFSample> sample;
    if (transform_allocates) {
      sample.Attach(output.pSample);
    } else {
      sample = allocated;
    }
    if (!sample) {
      return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
    }

    int64_t sample_time = 0;
    sample->GetSampleTime(&sample_time);

    ComPtr<IMFMediaBuffer> contiguous;
    hr = sample->ConvertToContiguousBuffer(&contiguous);
    if (FAILED(hr)) {
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }
    BYTE* data = nullptr;
    DWORD max_length = 0;
    DWORD current_length = 0;
    hr = contiguous->Lock(&data, &max_length, &current_length);
    if (FAILED(hr)) {
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }
    packet_.assign(data, data + current_length);
    contiguous->Unlock();

    pending_output_info_ = TakePendingInfo(sample_time);
    return ProcessEncodedFrame(packet_);
  }

  RTC_LOG(LS_ERROR) << "Encoder output stream change did not settle.";
  ReportError();
  return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
}

MFH264EncoderImpl::PendingFrameInfo MFH264EncoderImpl::TakePendingInfo(
    int64_t sample_time_100ns) {
  // With B-frames disabled output order matches input order; the sample time
  // check only guards against a driver dropping frames internally.
  while (!pending_frames_.empty() &&
         pending_frames_.front().sample_time_100ns < sample_time_100ns) {
    RTC_LOG(LS_WARNING) << "Encoder MFT dropped frame with sample time "
                        << pending_frames_.front().sample_time_100ns;
    pending_frames_.pop_front();
  }
  PendingFrameInfo info;
  if (!pending_frames_.empty()) {
    info = pending_frames_.front();
    pending_frames_.pop_front();
  } else {
    RTC_LOG(LS_WARNING) << "Encoded output without pending frame metadata.";
    info.sample_time_100ns = sample_time_100ns;
  }
  return info;
}

int32_t MFH264EncoderImpl::ProcessEncodedFrame(std::vector<uint8_t>& packet) {
  const PendingFrameInfo& info = pending_output_info_;

  bool is_idr = false;
  bool has_sps = false;
  std::vector<H264::NaluIndex> nalu_indices =
      H264::FindNaluIndices(MakeArrayView(packet.data(), packet.size()));
  for (const H264::NaluIndex& index : nalu_indices) {
    const H264::NaluType nalu_type =
        H264::ParseNaluType(packet[index.payload_start_offset]);
    if (nalu_type == H264::kIdr) {
      is_idr = true;
    } else if (nalu_type == H264::kSps) {
      has_sps = true;
    }
  }

  // Some vendor MFTs do not repeat SPS/PPS on every IDR; the RTP packetizer
  // needs them inline, so prepend the cached sequence header.
  if (is_idr && !has_sps) {
    if (sequence_header_.empty()) {
      CacheSequenceHeader();
    }
    if (!sequence_header_.empty()) {
      packet.insert(packet.begin(), sequence_header_.begin(),
                    sequence_header_.end());
    } else {
      RTC_LOG(LS_WARNING)
          << "IDR frame without SPS/PPS and no cached sequence header.";
    }
  }

  encoded_image_._encodedWidth = configuration_.width;
  encoded_image_._encodedHeight = configuration_.height;
  encoded_image_.SetRtpTimestamp(info.rtp_timestamp);
  encoded_image_.SetSimulcastIndex(0);
  encoded_image_.ntp_time_ms_ = info.ntp_time_ms;
  encoded_image_.capture_time_ms_ = info.render_time_ms;
  encoded_image_.rotation_ = info.rotation;
  encoded_image_.content_type_ = VideoContentType::UNSPECIFIED;
  encoded_image_.timing_.flags = VideoSendTiming::kInvalid;
  encoded_image_._frameType = is_idr ? VideoFrameType::kVideoFrameKey
                                     : VideoFrameType::kVideoFrameDelta;
  encoded_image_.SetColorSpace(info.color_space);

  encoded_image_.SetEncodedData(
      EncodedImageBuffer::Create(packet.data(), packet.size()));
  encoded_image_.set_size(packet.size());

  h264_bitstream_parser_.ParseBitstream(encoded_image_);
  encoded_image_.qp_ = h264_bitstream_parser_.GetLastSliceQp().value_or(-1);

  CodecSpecificInfo codec_info;
  codec_info.codecType = kVideoCodecH264;
  codec_info.codecSpecific.H264.packetization_mode =
      H264PacketizationMode::NonInterleaved;

  const auto result =
      encoded_image_callback_->OnEncodedImage(encoded_image_, &codec_info);
  if (result.error != EncodedImageCallback::Result::OK) {
    RTC_LOG(LS_ERROR) << "OnEncodedImage failed " << result.error;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

VideoEncoder::EncoderInfo MFH264EncoderImpl::GetEncoderInfo() const {
  EncoderInfo info;
  info.supports_native_handle = false;
  info.implementation_name = friendly_name_.empty()
                                 ? "MediaFoundation H264 Encoder"
                                 : "MediaFoundation H264 Encoder (" +
                                       friendly_name_ + ")";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  info.preferred_pixel_formats = {VideoFrameBuffer::Type::kI420};
  return info;
}

void MFH264EncoderImpl::ApplyBitrate(uint32_t bitrate_bps) {
  if (bitrate_bps == active_bitrate_bps_) {
    return;
  }
  if (dynamic_bitrate_supported_) {
    HRESULT hr = SetCodecApiUInt32(
        codec_api_.Get(), CODECAPI_AVEncCommonMeanBitRate, bitrate_bps);
    if (SUCCEEDED(hr)) {
      active_bitrate_bps_ = bitrate_bps;
      return;
    }
    dynamic_bitrate_supported_ = false;
    RTC_LOG(LS_WARNING)
        << "Encoder MFT rejected runtime bitrate update ("
        << HResultToString(hr)
        << "); falling back to re-init with hysteresis.";
  }
  // Dynamic update unsupported: re-initialize, but only when the target has
  // moved enough to matter — a full re-init costs a keyframe.
  const uint32_t reference = std::max(active_bitrate_bps_, 1u);
  const uint32_t delta = bitrate_bps > active_bitrate_bps_
                             ? bitrate_bps - active_bitrate_bps_
                             : active_bitrate_bps_ - bitrate_bps;
  if (delta * 5 > reference) {  // > 20% change
    pending_bitrate_reinit_ = true;
  }
}

void MFH264EncoderImpl::SetRates(const RateControlParameters& parameters) {
  if (!transform_) {
    RTC_LOG(LS_WARNING) << "SetRates() while uninitialized.";
    return;
  }

  if (parameters.framerate_fps < 1.0) {
    RTC_LOG(LS_WARNING) << "Invalid frame rate: " << parameters.framerate_fps;
    return;
  }

  if (parameters.bitrate.get_sum_bps() == 0) {
    configuration_.SetStreamState(false);
    return;
  }

  codec_.maxFramerate = static_cast<uint32_t>(parameters.framerate_fps);
  codec_.maxBitrate = parameters.bitrate.GetSpatialLayerSum(0);

  configuration_.target_bps = parameters.bitrate.GetSpatialLayerSum(0);
  configuration_.max_frame_rate = parameters.framerate_fps;

  if (configuration_.target_bps) {
    ApplyBitrate(configuration_.target_bps);
    configuration_.SetStreamState(true);
  } else {
    configuration_.SetStreamState(false);
  }
}

void MFH264EncoderImpl::LayerConfig::SetStreamState(bool send_stream) {
  if (send_stream && !sending) {
    // Need a key frame if we have not sent this stream before.
    key_frame_request = true;
  }
  sending = send_stream;
}

}  // namespace webrtc
