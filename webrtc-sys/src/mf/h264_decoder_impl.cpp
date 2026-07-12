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

#include "h264_decoder_impl.h"

#include <codecapi.h>
#include <d3d10.h>
#include <wmcodecdsp.h>

#include <algorithm>
#include <cstring>

#include <api/video/i420_buffer.h>
#include <api/video/video_codec_type.h>
#include <modules/video_coding/include/video_error_codes.h>
#include <third_party/libyuv/include/libyuv/convert.h>

#include "rtc_base/checks.h"
#include "rtc_base/logging.h"

namespace webrtc {

using livekit_ffi::ComPtr;
using livekit_ffi::HResultToString;

namespace {

// RTP video timestamps run at 90 kHz; MF sample times are 100 ns units. The
// timestamp is only used to carry the RTP value through the transform, so the
// conversion just needs to round-trip exactly for 32-bit inputs.
int64_t RtpToSampleTime(uint32_t rtp_timestamp) {
  return static_cast<int64_t>(rtp_timestamp) * 1000 / 9;
}

uint32_t SampleTimeToRtp(int64_t sample_time) {
  return static_cast<uint32_t>((sample_time * 9 + 500) / 1000);
}

}  // namespace

MFH264DecoderImpl::MFH264DecoderImpl() : buffer_pool_(false) {}

MFH264DecoderImpl::~MFH264DecoderImpl() {
  Release();
}

VideoDecoder::DecoderInfo MFH264DecoderImpl::GetDecoderInfo() const {
  VideoDecoder::DecoderInfo info;
  info.implementation_name = use_d3d_
                                 ? "MediaFoundation H264 Decoder (DXVA)"
                                 : "MediaFoundation H264 Decoder (software)";
  info.is_hardware_accelerated = use_d3d_;
  return info;
}

HRESULT MFH264DecoderImpl::SetupD3D() {
  UINT flags = D3D11_CREATE_DEVICE_VIDEO_SUPPORT;
  const D3D_FEATURE_LEVEL feature_levels[] = {
      D3D_FEATURE_LEVEL_11_1,
      D3D_FEATURE_LEVEL_11_0,
      D3D_FEATURE_LEVEL_10_1,
      D3D_FEATURE_LEVEL_10_0,
  };
  HRESULT hr = D3D11CreateDevice(
      nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, flags, feature_levels,
      ARRAYSIZE(feature_levels), D3D11_SDK_VERSION, &d3d_device_, nullptr,
      &d3d_context_);
  if (FAILED(hr)) {
    return hr;
  }

  // The decoder MFT accesses the device from its own threads.
  ComPtr<ID3D10Multithread> multithread;
  hr = d3d_device_.As(&multithread);
  if (FAILED(hr)) {
    return hr;
  }
  multithread->SetMultithreadProtected(TRUE);

  UINT reset_token = 0;
  hr = MFCreateDXGIDeviceManager(&reset_token, &dxgi_manager_);
  if (FAILED(hr)) {
    return hr;
  }
  hr = dxgi_manager_->ResetDevice(d3d_device_.Get(), reset_token);
  if (FAILED(hr)) {
    return hr;
  }

  hr = transform_->ProcessMessage(
      MFT_MESSAGE_SET_D3D_MANAGER,
      reinterpret_cast<ULONG_PTR>(dxgi_manager_.Get()));
  return hr;
}

bool MFH264DecoderImpl::Configure(const Settings& settings) {
  if (settings.codec_type() != kVideoCodecH264) {
    RTC_LOG(LS_ERROR)
        << "initialization failed on codectype is not kVideoCodecH264";
    return false;
  }
  if (!settings.max_render_resolution().Valid()) {
    RTC_LOG(LS_ERROR)
        << "initialization failed on codec_settings width < 0 or height < 0";
    return false;
  }

  settings_ = settings;

  if (!livekit_ffi::EnsureComInitialized() || !livekit_ffi::EnsureMFStarted()) {
    return false;
  }

  HRESULT hr =
      CoCreateInstance(CLSID_CMSH264DecoderMFT, nullptr, CLSCTX_INPROC_SERVER,
                       IID_PPV_ARGS(&transform_));
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "Failed to create inbox H264 decoder MFT: "
                      << HResultToString(hr);
    return false;
  }

  DWORD input_ids[1] = {0};
  DWORD output_ids[1] = {0};
  hr = transform_->GetStreamIDs(1, input_ids, 1, output_ids);
  if (hr == E_NOTIMPL) {
    input_ids[0] = 0;
    output_ids[0] = 0;
  } else if (FAILED(hr)) {
    return false;
  }
  input_stream_id_ = input_ids[0];
  output_stream_id_ = output_ids[0];

  // Without the D3D manager the inbox MFT decodes in software; that still
  // works, so treat D3D failure (headless boxes, RDP sessions, broken
  // drivers) as a downgrade rather than an error.
  hr = SetupD3D();
  use_d3d_ = SUCCEEDED(hr);
  if (!use_d3d_) {
    RTC_LOG(LS_WARNING)
        << "D3D11 unavailable for H264 decode, falling back to software: "
        << HResultToString(hr);
    d3d_device_.Reset();
    d3d_context_.Reset();
    dxgi_manager_.Reset();
  }

  // Cap internal reordering/buffering so frames come out ~1-in/1-out.
  ComPtr<IMFAttributes> attributes;
  if (SUCCEEDED(transform_->GetAttributes(&attributes)) && attributes) {
    attributes->SetUINT32(MF_LOW_LATENCY, TRUE);
  }
  ComPtr<ICodecAPI> codec_api;
  if (SUCCEEDED(transform_.As(&codec_api))) {
    VARIANT v = {};
    v.vt = VT_BOOL;
    v.boolVal = VARIANT_TRUE;
    codec_api->SetValue(&CODECAPI_AVLowLatencyMode, &v);
  }

  ComPtr<IMFMediaType> input_type;
  hr = MFCreateMediaType(&input_type);
  if (FAILED(hr)) {
    return false;
  }
  input_type->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
  input_type->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_H264);
  const RenderResolution& resolution = settings.max_render_resolution();
  MFSetAttributeSize(input_type.Get(), MF_MT_FRAME_SIZE,
                     static_cast<UINT32>(resolution.Width()),
                     static_cast<UINT32>(resolution.Height()));
  hr = transform_->SetInputType(input_stream_id_, input_type.Get(), 0);
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "Decoder SetInputType failed: "
                      << HResultToString(hr);
    return false;
  }

  hr = NegotiateOutputType();
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "Decoder output negotiation failed: "
                      << HResultToString(hr);
    return false;
  }

  transform_->ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
  transform_->ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0);

  RTC_LOG(LS_INFO) << "MediaFoundation H264 decoder initialized ("
                   << (use_d3d_ ? "DXVA hardware" : "software") << ").";
  return true;
}

HRESULT MFH264DecoderImpl::NegotiateOutputType() {
  HRESULT hr = E_FAIL;
  for (DWORD i = 0;; i++) {
    ComPtr<IMFMediaType> candidate;
    hr = transform_->GetOutputAvailableType(output_stream_id_, i, &candidate);
    if (FAILED(hr)) {
      return hr;
    }
    GUID subtype = GUID_NULL;
    candidate->GetGUID(MF_MT_SUBTYPE, &subtype);
    if (subtype != MFVideoFormat_NV12) {
      continue;
    }
    hr = transform_->SetOutputType(output_stream_id_, candidate.Get(), 0);
    if (FAILED(hr)) {
      return hr;
    }
    break;
  }

  ComPtr<IMFMediaType> current;
  hr = transform_->GetOutputCurrentType(output_stream_id_, &current);
  if (FAILED(hr)) {
    return hr;
  }
  hr = MFGetAttributeSize(current.Get(), MF_MT_FRAME_SIZE, &coded_width_,
                          &coded_height_);
  if (FAILED(hr)) {
    return hr;
  }

  // 1080p decodes as 1920x1088 coded size; the display aperture carries the
  // real dimensions.
  has_aperture_ =
      SUCCEEDED(current->GetBlob(MF_MT_MINIMUM_DISPLAY_APERTURE,
                                 reinterpret_cast<UINT8*>(&display_aperture_),
                                 sizeof(display_aperture_), nullptr));

  UINT32 stride = 0;
  if (SUCCEEDED(current->GetUINT32(MF_MT_DEFAULT_STRIDE, &stride)) &&
      static_cast<INT32>(stride) > 0) {
    default_stride_ = stride;
  } else {
    default_stride_ = coded_width_;
  }

  // Coded size may have changed; the staging texture is recreated lazily.
  staging_texture_.Reset();
  return S_OK;
}

int32_t MFH264DecoderImpl::RegisterDecodeCompleteCallback(
    DecodedImageCallback* callback) {
  this->decoded_complete_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MFH264DecoderImpl::Release() {
  if (transform_) {
    transform_->ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM,
                               input_stream_id_);
    transform_->ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
    transform_->ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
    transform_->ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, 0);
  }
  staging_texture_.Reset();
  transform_.Reset();
  dxgi_manager_.Reset();
  d3d_context_.Reset();
  d3d_device_.Reset();
  use_d3d_ = false;
  buffer_pool_.Release();
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MFH264DecoderImpl::Decode(const EncodedImage& input_image,
                                  bool missing_frames,
                                  int64_t render_time_ms) {
  if (!transform_) {
    RTC_LOG(LS_ERROR) << "decode failed: decoder not configured";
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (decoded_complete_callback_ == nullptr) {
    RTC_LOG(LS_ERROR) << "decode failed on not set decoded_complete_callback";
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!input_image.data() || !input_image.size()) {
    RTC_LOG(LS_ERROR) << "decode failed on input image is null";
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  h264_bitstream_parser_.ParseBitstream(input_image);
  std::optional<int> qp = h264_bitstream_parser_.GetLastSliceQp();

  // Pass on color space from the input frame if explicitly specified.
  if (input_image.ColorSpace()) {
    input_color_space_ = *input_image.ColorSpace();
  }

  ComPtr<IMFMediaBuffer> buffer;
  HRESULT hr = MFCreateMemoryBuffer(
      static_cast<DWORD>(input_image.size()), &buffer);
  if (FAILED(hr)) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  BYTE* data = nullptr;
  hr = buffer->Lock(&data, nullptr, nullptr);
  if (FAILED(hr)) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  std::memcpy(data, input_image.data(), input_image.size());
  buffer->Unlock();
  buffer->SetCurrentLength(static_cast<DWORD>(input_image.size()));

  ComPtr<IMFSample> sample;
  hr = MFCreateSample(&sample);
  if (FAILED(hr)) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  sample->AddBuffer(buffer.Get());
  sample->SetSampleTime(RtpToSampleTime(input_image.RtpTimestamp()));

  for (int attempt = 0; attempt < 2; attempt++) {
    hr = transform_->ProcessInput(input_stream_id_, sample.Get(), 0);
    if (hr != MF_E_NOTACCEPTING) {
      break;
    }
    // The transform is full; empty it and try once more.
    int32_t ret = DrainOutputs(qp);
    if (ret != WEBRTC_VIDEO_CODEC_OK) {
      return ret;
    }
  }
  if (FAILED(hr)) {
    RTC_LOG(LS_ERROR) << "Decoder ProcessInput failed: "
                      << HResultToString(hr);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  return DrainOutputs(qp);
}

int32_t MFH264DecoderImpl::DrainOutputs(std::optional<int> qp) {
  for (;;) {
    MFT_OUTPUT_STREAM_INFO stream_info = {};
    HRESULT hr =
        transform_->GetOutputStreamInfo(output_stream_id_, &stream_info);
    if (FAILED(hr)) {
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    const bool transform_allocates =
        stream_info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES |
                               MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES);

    ComPtr<IMFSample> allocated;
    if (!transform_allocates) {
      hr = MFCreateSample(&allocated);
      if (FAILED(hr)) {
        return WEBRTC_VIDEO_CODEC_ERROR;
      }
      ComPtr<IMFMediaBuffer> out_buffer;
      const DWORD size =
          stream_info.cbSize
              ? stream_info.cbSize
              : coded_width_ * coded_height_ * 3 / 2;
      hr = MFCreateAlignedMemoryBuffer(
          size, stream_info.cbAlignment > 1 ? stream_info.cbAlignment - 1 : 0,
          &out_buffer);
      if (FAILED(hr)) {
        return WEBRTC_VIDEO_CODEC_ERROR;
      }
      allocated->AddBuffer(out_buffer.Get());
    }

    MFT_OUTPUT_DATA_BUFFER output = {};
    output.dwStreamID = output_stream_id_;
    output.pSample = transform_allocates ? nullptr : allocated.Get();
    DWORD status = 0;
    hr = transform_->ProcessOutput(0, 1, &output, &status);
    if (output.pEvents) {
      output.pEvents->Release();
    }

    if (hr == MF_E_TRANSFORM_NEED_MORE_INPUT) {
      return WEBRTC_VIDEO_CODEC_OK;
    }
    if (hr == MF_E_TRANSFORM_STREAM_CHANGE) {
      HRESULT nhr = NegotiateOutputType();
      if (FAILED(nhr)) {
        RTC_LOG(LS_ERROR) << "Decoder output renegotiation failed: "
                          << HResultToString(nhr);
        return WEBRTC_VIDEO_CODEC_ERROR;
      }
      continue;
    }
    if (FAILED(hr)) {
      RTC_LOG(LS_ERROR) << "Decoder ProcessOutput failed: "
                        << HResultToString(hr);
      return WEBRTC_VIDEO_CODEC_ERROR;
    }

    ComPtr<IMFSample> sample;
    if (transform_allocates) {
      sample.Attach(output.pSample);
    } else {
      sample = allocated;
    }
    if (!sample) {
      continue;
    }
    int32_t ret = DeliverSample(sample.Get(), qp);
    if (ret != WEBRTC_VIDEO_CODEC_OK) {
      return ret;
    }
  }
}

int32_t MFH264DecoderImpl::DeliverSample(IMFSample* sample,
                                         std::optional<int> qp) {
  int64_t sample_time = 0;
  sample->GetSampleTime(&sample_time);
  const uint32_t rtp_timestamp = SampleTimeToRtp(sample_time);

  ComPtr<IMFMediaBuffer> buffer;
  HRESULT hr = sample->GetBufferByIndex(0, &buffer);
  if (FAILED(hr)) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // Crop offsets within the coded frame (1088-line 1080p etc.).
  const UINT32 crop_x = has_aperture_ ? display_aperture_.OffsetX.value : 0;
  const UINT32 crop_y = has_aperture_ ? display_aperture_.OffsetY.value : 0;

  ComPtr<IMFDXGIBuffer> dxgi_buffer;
  if (use_d3d_ && SUCCEEDED(buffer.As(&dxgi_buffer))) {
    ComPtr<ID3D11Texture2D> texture;
    hr = dxgi_buffer->GetResource(IID_PPV_ARGS(&texture));
    if (FAILED(hr)) {
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    UINT subresource = 0;
    dxgi_buffer->GetSubresourceIndex(&subresource);

    D3D11_TEXTURE2D_DESC desc = {};
    texture->GetDesc(&desc);

    if (staging_texture_) {
      D3D11_TEXTURE2D_DESC staging_desc = {};
      staging_texture_->GetDesc(&staging_desc);
      if (staging_desc.Width != desc.Width ||
          staging_desc.Height != desc.Height) {
        staging_texture_.Reset();
      }
    }
    if (!staging_texture_) {
      D3D11_TEXTURE2D_DESC staging_desc = {};
      staging_desc.Width = desc.Width;
      staging_desc.Height = desc.Height;
      staging_desc.MipLevels = 1;
      staging_desc.ArraySize = 1;
      staging_desc.Format = DXGI_FORMAT_NV12;
      staging_desc.SampleDesc.Count = 1;
      staging_desc.Usage = D3D11_USAGE_STAGING;
      staging_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
      hr = d3d_device_->CreateTexture2D(&staging_desc, nullptr,
                                        &staging_texture_);
      if (FAILED(hr)) {
        RTC_LOG(LS_ERROR) << "Failed to create staging texture: "
                          << HResultToString(hr);
        return WEBRTC_VIDEO_CODEC_ERROR;
      }
    }

    // GPU -> CPU readback: the known copy-back cost of this design.
    d3d_context_->CopySubresourceRegion(staging_texture_.Get(), 0, 0, 0, 0,
                                        texture.Get(), subresource, nullptr);
    D3D11_MAPPED_SUBRESOURCE mapped = {};
    hr = d3d_context_->Map(staging_texture_.Get(), 0, D3D11_MAP_READ, 0,
                           &mapped);
    if (FAILED(hr)) {
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    const uint8_t* base = static_cast<const uint8_t*>(mapped.pData);
    const uint8_t* data_y = base + crop_y * mapped.RowPitch + crop_x;
    const uint8_t* data_uv = base +
                             static_cast<size_t>(mapped.RowPitch) *
                                 desc.Height +
                             (crop_y / 2) * mapped.RowPitch + crop_x;
    int32_t ret = DeliverNV12(data_y, mapped.RowPitch, data_uv,
                              mapped.RowPitch, rtp_timestamp, qp);
    d3d_context_->Unmap(staging_texture_.Get(), 0);
    return ret;
  }

  // Software path: system-memory NV12.
  ComPtr<IMF2DBuffer> buffer_2d;
  if (SUCCEEDED(buffer.As(&buffer_2d))) {
    BYTE* scanline0 = nullptr;
    LONG pitch = 0;
    hr = buffer_2d->Lock2D(&scanline0, &pitch);
    if (FAILED(hr) || pitch <= 0) {
      if (SUCCEEDED(hr)) {
        buffer_2d->Unlock2D();
      }
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    const uint8_t* data_y = scanline0 + crop_y * pitch + crop_x;
    const uint8_t* data_uv = scanline0 +
                             static_cast<size_t>(pitch) * coded_height_ +
                             (crop_y / 2) * pitch + crop_x;
    int32_t ret =
        DeliverNV12(data_y, pitch, data_uv, pitch, rtp_timestamp, qp);
    buffer_2d->Unlock2D();
    return ret;
  }

  ComPtr<IMFMediaBuffer> contiguous;
  hr = sample->ConvertToContiguousBuffer(&contiguous);
  if (FAILED(hr)) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  BYTE* data = nullptr;
  DWORD length = 0;
  hr = contiguous->Lock(&data, nullptr, &length);
  if (FAILED(hr)) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  const UINT32 stride = default_stride_;
  const uint8_t* data_y = data + crop_y * stride + crop_x;
  const uint8_t* data_uv = data + static_cast<size_t>(stride) * coded_height_ +
                           (crop_y / 2) * stride + crop_x;
  int32_t ret = DeliverNV12(data_y, stride, data_uv, stride, rtp_timestamp, qp);
  contiguous->Unlock();
  return ret;
}

int32_t MFH264DecoderImpl::DeliverNV12(const uint8_t* data_y,
                                       int stride_y,
                                       const uint8_t* data_uv,
                                       int stride_uv,
                                       uint32_t rtp_timestamp,
                                       std::optional<int> qp) {
  const int width = has_aperture_ ? display_aperture_.Area.cx : coded_width_;
  const int height = has_aperture_ ? display_aperture_.Area.cy : coded_height_;

  webrtc::scoped_refptr<webrtc::I420Buffer> i420_buffer =
      buffer_pool_.CreateI420Buffer(width, height);
  if (!i420_buffer) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  int result = libyuv::NV12ToI420(
      data_y, stride_y, data_uv, stride_uv, i420_buffer->MutableDataY(),
      i420_buffer->StrideY(), i420_buffer->MutableDataU(),
      i420_buffer->StrideU(), i420_buffer->MutableDataV(),
      i420_buffer->StrideV(), width, height);
  if (result) {
    RTC_LOG(LS_INFO) << "libyuv::NV12ToI420 failed. error:" << result;
  }

  VideoFrame decoded_frame = VideoFrame::Builder()
                                 .set_video_frame_buffer(i420_buffer)
                                 .set_timestamp_rtp(rtp_timestamp)
                                 .set_color_space(input_color_space_)
                                 .build();

  std::optional<int32_t> decode_time;
  decoded_complete_callback_->Decoded(decoded_frame, decode_time, qp);
  return WEBRTC_VIDEO_CODEC_OK;
}

}  // namespace webrtc
