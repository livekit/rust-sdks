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

#ifndef WEBRTC_MF_H264_DECODER_IMPL_H_
#define WEBRTC_MF_H264_DECODER_IMPL_H_

#include "mf_common.h"

#include <d3d11.h>

#include <optional>

#include <api/video/color_space.h>
#include <api/video_codecs/video_decoder.h>
#include <common_video/h264/h264_bitstream_parser.h>
#include <common_video/include/video_frame_buffer_pool.h>

namespace webrtc {

// H264 decoder on top of the Windows inbox decoder MFT
// (CLSID_CMSH264DecoderMFT). The inbox decoder fronts DXVA hardware decode
// for every GPU vendor, but only when it is handed a D3D11 device manager —
// without one it silently decodes in software on the CPU. Output samples wrap
// D3D11 textures which are copied back through a staging texture into I420
// for webrtc (same copy-back cost the NVDEC backend pays).
class MFH264DecoderImpl : public VideoDecoder {
 public:
  MFH264DecoderImpl();
  MFH264DecoderImpl(const MFH264DecoderImpl&) = delete;
  MFH264DecoderImpl& operator=(const MFH264DecoderImpl&) = delete;
  ~MFH264DecoderImpl() override;

  bool Configure(const Settings& settings) override;
  int32_t Decode(const EncodedImage& input_image,
                 bool missing_frames,
                 int64_t render_time_ms) override;
  int32_t RegisterDecodeCompleteCallback(
      DecodedImageCallback* callback) override;
  int32_t Release() override;
  DecoderInfo GetDecoderInfo() const override;

 private:
  HRESULT SetupD3D();
  // Selects an NV12 output type and refreshes coded size, display aperture
  // and stride. Also called on MF_E_TRANSFORM_STREAM_CHANGE (fires on
  // resolution change).
  HRESULT NegotiateOutputType();
  int32_t DrainOutputs(std::optional<int> qp);
  int32_t DeliverSample(IMFSample* sample, std::optional<int> qp);
  int32_t DeliverNV12(const uint8_t* data_y,
                      int stride_y,
                      const uint8_t* data_uv,
                      int stride_uv,
                      uint32_t rtp_timestamp,
                      std::optional<int> qp);

  livekit_ffi::ComPtr<IMFTransform> transform_;
  livekit_ffi::ComPtr<ID3D11Device> d3d_device_;
  livekit_ffi::ComPtr<ID3D11DeviceContext> d3d_context_;
  livekit_ffi::ComPtr<IMFDXGIDeviceManager> dxgi_manager_;
  livekit_ffi::ComPtr<ID3D11Texture2D> staging_texture_;
  bool use_d3d_ = false;
  DWORD input_stream_id_ = 0;
  DWORD output_stream_id_ = 0;

  UINT32 coded_width_ = 0;
  UINT32 coded_height_ = 0;
  MFVideoArea display_aperture_ = {};
  bool has_aperture_ = false;
  UINT32 default_stride_ = 0;

  Settings settings_;
  std::optional<webrtc::ColorSpace> input_color_space_;
  DecodedImageCallback* decoded_complete_callback_ = nullptr;
  webrtc::VideoFrameBufferPool buffer_pool_;
  webrtc::H264BitstreamParser h264_bitstream_parser_;
};

}  // namespace webrtc

#endif  // WEBRTC_MF_H264_DECODER_IMPL_H_
