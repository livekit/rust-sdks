/*
 * Copyright 2026 LiveKit, Inc.
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

#include "livekit/d3d11_video_frame_buffer.h"

#include <d3d11_4.h>  // ID3D11Multithread
#include <wrl/client.h>

#include <algorithm>
#include <atomic>
#include <cmath>
#include <cstring>
#include <ios>
#include <mutex>

#include "api/make_ref_counted.h"
#include "api/video/i420_buffer.h"
#include "api/video/nv12_buffer.h"
#include "rtc_base/logging.h"
#include "third_party/libyuv/include/libyuv/convert_from_argb.h"

namespace livekit {

using Microsoft::WRL::ComPtr;

namespace {

std::atomic<uint64_t> g_readbacks{0};

int Even(int v) {
  return (v + 1) & ~1;
}

// Holds the device's lock across a sequence of immediate-context calls, so a
// capture thread on the same device cannot interleave with a map.
class DeviceLock {
 public:
  explicit DeviceLock(ID3D11DeviceContext* context) {
    context->QueryInterface(IID_PPV_ARGS(&multithread_));
    if (multithread_) {
      multithread_->Enter();
    }
  }
  ~DeviceLock() {
    if (multithread_) {
      multithread_->Leave();
    }
  }

 private:
  ComPtr<ID3D11Multithread> multithread_;
};

// GPU BGRA -> NV12 for one device, one source texture size and one output
// size: the D3D11 video processor, its NV12 target and a staging copy. Or,
// when the device has no video processor, a BGRA staging texture for a CPU
// conversion. Rebuilt when any of the three changes.
class Converter {
 public:
  static std::unique_ptr<Converter> Create(ID3D11Device* device,
                                           int texture_width,
                                           int texture_height,
                                           int out_width,
                                           int out_height) {
    auto c = std::unique_ptr<Converter>(new Converter());
    c->device_ = device;
    device->GetImmediateContext(&c->context_);
    c->texture_width_ = texture_width;
    c->texture_height_ = texture_height;
    c->out_width_ = out_width;
    c->out_height_ = out_height;
    if (!c->InitVideoProcessor()) {
      c->video_device_.Reset();
      c->video_context_.Reset();
      c->enumerator_.Reset();
      c->processor_.Reset();
      c->nv12_.Reset();
      c->output_view_.Reset();
      c->staging_.Reset();
      RTC_LOG(LS_WARNING) << "D3D11VideoFrameBuffer: no video processor on "
                             "this device, reading BGRA back for the CPU";
    }
    return c;
  }

  bool Matches(ID3D11Device* device,
               int texture_width,
               int texture_height,
               int out_width,
               int out_height) const {
    return device_.Get() == device && texture_width_ == texture_width &&
           texture_height_ == texture_height && out_width_ == out_width &&
           out_height_ == out_height;
  }

  bool uses_video_processor() const { return processor_ != nullptr; }

  webrtc::scoped_refptr<webrtc::NV12Buffer> Read(
      ID3D11Texture2D* texture,
      const D3D11VideoFrameBuffer::Rect& rect) {
    DeviceLock lock(context_.Get());
    if (processor_) {
      D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC ivd = {};
      ivd.ViewDimension = D3D11_VPIV_DIMENSION_TEXTURE2D;
      ComPtr<ID3D11VideoProcessorInputView> input_view;
      HRESULT hr = video_device_->CreateVideoProcessorInputView(
          texture, enumerator_.Get(), &ivd, &input_view);
      if (SUCCEEDED(hr)) {
        return ReadNV12(input_view.Get(), rect);
      }
      // A texture the video processor cannot take (its bind flags, say)
      // still converts, on the CPU.
      if (!warned_input_view_) {
        warned_input_view_ = true;
        RTC_LOG(LS_WARNING) << "D3D11VideoFrameBuffer: video processor input "
                               "view failed (hr=0x"
                            << std::hex << hr
                            << "), reading BGRA back for the CPU";
      }
    }
    return ReadBGRA(texture, rect);
  }

 private:
  Converter() = default;

  bool InitVideoProcessor() {
    if (FAILED(device_.As(&video_device_)) ||
        FAILED(context_.As(&video_context_))) {
      return false;
    }
    D3D11_VIDEO_PROCESSOR_CONTENT_DESC desc = {};
    desc.InputFrameFormat = D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE;
    desc.InputWidth = texture_width_;
    desc.InputHeight = texture_height_;
    desc.OutputWidth = out_width_;
    desc.OutputHeight = out_height_;
    desc.Usage = D3D11_VIDEO_USAGE_OPTIMAL_SPEED;
    if (FAILED(video_device_->CreateVideoProcessorEnumerator(&desc,
                                                             &enumerator_))) {
      return false;
    }
    UINT flags = 0;
    if (FAILED(enumerator_->CheckVideoProcessorFormat(
            DXGI_FORMAT_B8G8R8A8_UNORM, &flags)) ||
        !(flags & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT)) {
      return false;
    }
    flags = 0;
    if (FAILED(enumerator_->CheckVideoProcessorFormat(DXGI_FORMAT_NV12,
                                                      &flags)) ||
        !(flags & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT)) {
      return false;
    }
    if (FAILED(video_device_->CreateVideoProcessor(enumerator_.Get(), 0,
                                                   &processor_))) {
      return false;
    }

    D3D11_TEXTURE2D_DESC td = {};
    td.Width = Even(out_width_);
    td.Height = Even(out_height_);
    td.MipLevels = 1;
    td.ArraySize = 1;
    td.Format = DXGI_FORMAT_NV12;
    td.SampleDesc.Count = 1;
    td.Usage = D3D11_USAGE_DEFAULT;
    td.BindFlags = D3D11_BIND_RENDER_TARGET;
    if (FAILED(device_->CreateTexture2D(&td, nullptr, &nv12_))) {
      return false;
    }
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC ovd = {};
    ovd.ViewDimension = D3D11_VPOV_DIMENSION_TEXTURE2D;
    if (FAILED(video_device_->CreateVideoProcessorOutputView(
            nv12_.Get(), enumerator_.Get(), &ovd, &output_view_))) {
      return false;
    }
    td.Usage = D3D11_USAGE_STAGING;
    td.BindFlags = 0;
    td.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    if (FAILED(device_->CreateTexture2D(&td, nullptr, &staging_))) {
      return false;
    }

    // Full-range RGB in, BT.601 limited-range YCbCr out: what libyuv's
    // ARGBToI420 produces and what a VP8 receiver assumes.
    D3D11_VIDEO_PROCESSOR_COLOR_SPACE in_space = {};
    in_space.RGB_Range = 0;  // 0-255
    D3D11_VIDEO_PROCESSOR_COLOR_SPACE out_space = {};
    out_space.YCbCr_Matrix = 0;  // BT.601
    out_space.Nominal_Range = D3D11_VIDEO_PROCESSOR_NOMINAL_RANGE_16_235;
    video_context_->VideoProcessorSetStreamColorSpace(processor_.Get(), 0,
                                                      &in_space);
    video_context_->VideoProcessorSetOutputColorSpace(processor_.Get(),
                                                      &out_space);
    video_context_->VideoProcessorSetStreamFrameFormat(
        processor_.Get(), 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
    video_context_->VideoProcessorSetStreamAutoProcessingMode(processor_.Get(),
                                                              0, FALSE);
    return true;
  }

  webrtc::scoped_refptr<webrtc::NV12Buffer> ReadNV12(
      ID3D11VideoProcessorInputView* input_view,
      const D3D11VideoFrameBuffer::Rect& rect) {
    RECT src = {rect.x, rect.y, rect.x + rect.width, rect.y + rect.height};
    RECT dst = {0, 0, out_width_, out_height_};
    video_context_->VideoProcessorSetStreamSourceRect(processor_.Get(), 0, TRUE,
                                                      &src);
    video_context_->VideoProcessorSetStreamDestRect(processor_.Get(), 0, TRUE,
                                                    &dst);
    video_context_->VideoProcessorSetOutputTargetRect(processor_.Get(), TRUE,
                                                      &dst);
    D3D11_VIDEO_PROCESSOR_STREAM stream = {};
    stream.Enable = TRUE;
    stream.pInputSurface = input_view;
    HRESULT hr = video_context_->VideoProcessorBlt(processor_.Get(), output_view_.Get(),
                                           0, 1, &stream);
    if (FAILED(hr)) {
      RTC_LOG(LS_ERROR) << "D3D11VideoFrameBuffer: VideoProcessorBlt failed, "
                           "hr=0x"
                        << std::hex << hr;
      return nullptr;
    }
    context_->CopyResource(staging_.Get(), nv12_.Get());

    D3D11_MAPPED_SUBRESOURCE mapped = {};
    hr = context_->Map(staging_.Get(), 0, D3D11_MAP_READ, 0, &mapped);
    if (FAILED(hr)) {
      RTC_LOG(LS_ERROR) << "D3D11VideoFrameBuffer: Map failed, hr=0x"
                        << std::hex << hr;
      return nullptr;
    }
    g_readbacks.fetch_add(1, std::memory_order_relaxed);

    auto out = webrtc::NV12Buffer::Create(out_width_, out_height_);
    const uint8_t* src_y = static_cast<const uint8_t*>(mapped.pData);
    // The UV plane follows the Y plane's rows (the texture's height, even).
    const uint8_t* src_uv = src_y + mapped.RowPitch * Even(out_height_);
    for (int row = 0; row < out_height_; ++row) {
      std::memcpy(out->MutableDataY() + row * out->StrideY(),
                  src_y + row * mapped.RowPitch, out_width_);
    }
    const int uv_bytes = Even(out_width_);
    for (int row = 0; row < (out_height_ + 1) / 2; ++row) {
      std::memcpy(out->MutableDataUV() + row * out->StrideUV(),
                  src_uv + row * mapped.RowPitch, uv_bytes);
    }
    context_->Unmap(staging_.Get(), 0);
    return out;
  }

  webrtc::scoped_refptr<webrtc::NV12Buffer> ReadBGRA(
      ID3D11Texture2D* texture,
      const D3D11VideoFrameBuffer::Rect& rect) {
    if (!staging_bgra_ || staging_size_ != std::make_pair(rect.width,
                                                          rect.height)) {
      D3D11_TEXTURE2D_DESC td = {};
      td.Width = rect.width;
      td.Height = rect.height;
      td.MipLevels = 1;
      td.ArraySize = 1;
      td.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
      td.SampleDesc.Count = 1;
      td.Usage = D3D11_USAGE_STAGING;
      td.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
      staging_bgra_.Reset();
      if (FAILED(device_->CreateTexture2D(&td, nullptr, &staging_bgra_))) {
        RTC_LOG(LS_ERROR) << "D3D11VideoFrameBuffer: staging texture failed";
        return nullptr;
      }
      staging_size_ = {rect.width, rect.height};
    }
    D3D11_BOX box = {static_cast<UINT>(rect.x),
                     static_cast<UINT>(rect.y),
                     0,
                     static_cast<UINT>(rect.x + rect.width),
                     static_cast<UINT>(rect.y + rect.height),
                     1};
    context_->CopySubresourceRegion(staging_bgra_.Get(), 0, 0, 0, 0, texture,
                                    0, &box);
    D3D11_MAPPED_SUBRESOURCE mapped = {};
    HRESULT hr =
        context_->Map(staging_bgra_.Get(), 0, D3D11_MAP_READ, 0, &mapped);
    if (FAILED(hr)) {
      RTC_LOG(LS_ERROR) << "D3D11VideoFrameBuffer: Map failed, hr=0x"
                        << std::hex << hr;
      return nullptr;
    }
    g_readbacks.fetch_add(1, std::memory_order_relaxed);

    // libyuv's "ARGB" is B, G, R, A in memory: DXGI's BGRA.
    auto full = webrtc::NV12Buffer::Create(rect.width, rect.height);
    libyuv::ARGBToNV12(static_cast<const uint8_t*>(mapped.pData),
                       mapped.RowPitch, full->MutableDataY(), full->StrideY(),
                       full->MutableDataUV(), full->StrideUV(), rect.width,
                       rect.height);
    context_->Unmap(staging_bgra_.Get(), 0);
    if (rect.width == out_width_ && rect.height == out_height_) {
      return full;
    }
    auto out = webrtc::NV12Buffer::Create(out_width_, out_height_);
    out->CropAndScaleFrom(*full, 0, 0, rect.width, rect.height);
    return out;
  }

  ComPtr<ID3D11Device> device_;
  ComPtr<ID3D11DeviceContext> context_;
  int texture_width_ = 0;
  int texture_height_ = 0;
  int out_width_ = 0;
  int out_height_ = 0;

  ComPtr<ID3D11VideoDevice> video_device_;
  ComPtr<ID3D11VideoContext> video_context_;
  ComPtr<ID3D11VideoProcessorEnumerator> enumerator_;
  ComPtr<ID3D11VideoProcessor> processor_;
  ComPtr<ID3D11Texture2D> nv12_;
  ComPtr<ID3D11VideoProcessorOutputView> output_view_;
  ComPtr<ID3D11Texture2D> staging_;

  ComPtr<ID3D11Texture2D> staging_bgra_;
  bool warned_input_view_ = false;
  std::pair<int, int> staging_size_{0, 0};
};

// One converter for the process: a sharer has one capture device at a time.
// It holds a reference to the last device used until a frame from another
// device, or of another size, replaces it.
std::mutex g_converter_mutex;
std::unique_ptr<Converter> g_converter;

webrtc::scoped_refptr<webrtc::NV12Buffer> ReadBack(
    ID3D11Texture2D* texture,
    const D3D11VideoFrameBuffer::Rect& rect,
    int out_width,
    int out_height) {
  ComPtr<ID3D11Device> device;
  texture->GetDevice(&device);
  D3D11_TEXTURE2D_DESC desc = {};
  texture->GetDesc(&desc);
  std::lock_guard<std::mutex> lock(g_converter_mutex);
  if (!g_converter ||
      !g_converter->Matches(device.Get(), desc.Width, desc.Height, out_width,
                            out_height)) {
    g_converter.reset();
    g_converter = Converter::Create(device.Get(), desc.Width, desc.Height,
                                    out_width, out_height);
  }
  return g_converter->Read(texture, rect);
}

}  // namespace

// What every view of one captured frame shares: the texture, the caller's
// keep-alive, and the NV12 readback once there is one.
struct D3D11VideoFrameBuffer::Frame {
  ComPtr<ID3D11Texture2D> texture;
  std::shared_ptr<void> keep_alive;

  std::mutex mutex;
  Rect cached_rect;
  webrtc::scoped_refptr<webrtc::NV12Buffer> cached;
};

webrtc::scoped_refptr<D3D11VideoFrameBuffer> D3D11VideoFrameBuffer::Create(
    ID3D11Texture2D* texture,
    int width,
    int height,
    std::shared_ptr<void> keep_alive) {
  if (!texture || width <= 0 || height <= 0) {
    return nullptr;
  }
  D3D11_TEXTURE2D_DESC desc = {};
  texture->GetDesc(&desc);
  if (desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM ||
      static_cast<UINT>(width) > desc.Width ||
      static_cast<UINT>(height) > desc.Height || desc.SampleDesc.Count != 1) {
    RTC_LOG(LS_ERROR) << "D3D11VideoFrameBuffer: unsupported texture (format "
                      << desc.Format << ", " << desc.Width << "x"
                      << desc.Height << ") for " << width << "x" << height;
    return nullptr;
  }
  auto frame = std::make_shared<Frame>();
  frame->texture = texture;
  frame->keep_alive = std::move(keep_alive);
  return webrtc::make_ref_counted<D3D11VideoFrameBuffer>(
      std::move(frame), Rect{0, 0, width, height}, width, height);
}

D3D11VideoFrameBuffer::D3D11VideoFrameBuffer(std::shared_ptr<Frame> frame,
                                             Rect source_rect,
                                             int width,
                                             int height)
    : frame_(std::move(frame)),
      source_rect_(source_rect),
      width_(width),
      height_(height) {}

D3D11VideoFrameBuffer::~D3D11VideoFrameBuffer() = default;

webrtc::VideoFrameBuffer::Type D3D11VideoFrameBuffer::type() const {
  return Type::kNative;
}

int D3D11VideoFrameBuffer::width() const {
  return width_;
}

int D3D11VideoFrameBuffer::height() const {
  return height_;
}

ID3D11Texture2D* D3D11VideoFrameBuffer::texture() const {
  return frame_->texture.Get();
}

webrtc::scoped_refptr<webrtc::NV12BufferInterface>
D3D11VideoFrameBuffer::ToNV12() {
  std::lock_guard<std::mutex> lock(frame_->mutex);
  auto& cached = frame_->cached;
  if (cached && frame_->cached_rect == source_rect_ &&
      cached->width() >= width_ && cached->height() >= height_) {
    if (cached->width() == width_ && cached->height() == height_) {
      return cached;
    }
    auto scaled = webrtc::NV12Buffer::Create(width_, height_);
    scaled->CropAndScaleFrom(*cached, 0, 0, cached->width(), cached->height());
    return scaled;
  }
  auto read = ReadBack(frame_->texture.Get(), source_rect_, width_, height_);
  if (read && (!cached || read->width() * read->height() >=
                              cached->width() * cached->height())) {
    cached = read;
    frame_->cached_rect = source_rect_;
  }
  return read;
}

webrtc::scoped_refptr<webrtc::I420BufferInterface>
D3D11VideoFrameBuffer::ToI420() {
  auto nv12 = ToNV12();
  if (!nv12) {
    return nullptr;
  }
  return nv12->ToI420();
}

webrtc::scoped_refptr<webrtc::VideoFrameBuffer>
D3D11VideoFrameBuffer::GetMappedFrameBuffer(std::span<Type> types) {
  if (std::find(types.begin(), types.end(), Type::kNV12) != types.end()) {
    return ToNV12();
  }
  if (std::find(types.begin(), types.end(), Type::kI420) != types.end()) {
    return ToI420();
  }
  return nullptr;
}

webrtc::scoped_refptr<webrtc::VideoFrameBuffer>
D3D11VideoFrameBuffer::CropAndScale(int offset_x,
                                    int offset_y,
                                    int crop_width,
                                    int crop_height,
                                    int scaled_width,
                                    int scaled_height) {
  // Offsets are in this view's pixels; map them onto the texture.
  const double sx = static_cast<double>(source_rect_.width) / width_;
  const double sy = static_cast<double>(source_rect_.height) / height_;
  Rect rect{source_rect_.x + static_cast<int>(std::lround(offset_x * sx)),
            source_rect_.y + static_cast<int>(std::lround(offset_y * sy)),
            static_cast<int>(std::lround(crop_width * sx)),
            static_cast<int>(std::lround(crop_height * sy))};
  rect.width = std::clamp(rect.width, 1,
                          source_rect_.x + source_rect_.width - rect.x);
  rect.height = std::clamp(rect.height, 1,
                           source_rect_.y + source_rect_.height - rect.y);
  return webrtc::make_ref_counted<D3D11VideoFrameBuffer>(
      frame_, rect, scaled_width, scaled_height);
}

std::string D3D11VideoFrameBuffer::storage_representation() const {
  return "D3D11 BGRA texture";
}

D3D11VideoFrameBuffer* D3D11VideoFrameBuffer::FromNative(
    webrtc::VideoFrameBuffer* buffer) {
  if (!buffer || buffer->type() != webrtc::VideoFrameBuffer::Type::kNative) {
    return nullptr;
  }
  return dynamic_cast<D3D11VideoFrameBuffer*>(buffer);
}

uint64_t D3D11VideoFrameBuffer::readback_count() {
  return g_readbacks.load(std::memory_order_relaxed);
}

}  // namespace livekit
