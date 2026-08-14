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

#include "livekit/mpp_mjpeg_decoder.h"

#include <dlfcn.h>

#include <chrono>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <limits>
#include <new>
#include <thread>

#include <rockchip/mpp_buffer.h>
#include <rockchip/mpp_frame.h>
#include <rockchip/mpp_packet.h>
#include <rockchip/rk_mpi.h>

namespace {

constexpr uint32_t kMppAlignment = 16;
constexpr size_t kOutputBytesPerPixel = 4;
constexpr int kQueueAttempts = 5;
constexpr int kFrameAttempts = 20;

int SetError(char* error, size_t capacity, const char* format, ...) {
  if (error && capacity > 0) {
    va_list args;
    va_start(args, format);
    std::vsnprintf(error, capacity, format, args);
    va_end(args);
    error[capacity - 1] = '\0';
  }
  return -1;
}

void ClearError(char* error, size_t capacity) {
  if (error && capacity > 0) {
    error[0] = '\0';
  }
}

bool AlignUp(uint32_t value, uint32_t* aligned) {
  if (!aligned || value > std::numeric_limits<uint32_t>::max() -
                              (kMppAlignment - 1)) {
    return false;
  }
  *aligned = (value + kMppAlignment - 1) & ~(kMppAlignment - 1);
  return true;
}

bool CheckedOutputSize(uint32_t horizontal_stride,
                       uint32_t vertical_stride,
                       size_t* output_size) {
  if (!output_size || horizontal_stride == 0 || vertical_stride == 0) {
    return false;
  }
  const size_t horizontal = horizontal_stride;
  const size_t vertical = vertical_stride;
  if (horizontal > std::numeric_limits<size_t>::max() / vertical) {
    return false;
  }
  const size_t pixels = horizontal * vertical;
  if (pixels > std::numeric_limits<size_t>::max() / kOutputBytesPerPixel) {
    return false;
  }
  *output_size = pixels * kOutputBytesPerPixel;
  return true;
}

bool CheckDestination(size_t destination_size,
                      uint32_t destination_stride,
                      uint32_t row_bytes,
                      uint32_t rows) {
  if (row_bytes == 0 || rows == 0 || destination_stride < row_bytes) {
    return false;
  }
  const size_t last_row_offset =
      static_cast<size_t>(destination_stride) * (rows - 1);
  return last_row_offset <= destination_size &&
         row_bytes <= destination_size - last_row_offset;
}

bool HasRequiredMppSymbols(char* error, size_t error_capacity) {
  void* library = dlopen("librockchip_mpp.so", RTLD_LAZY | RTLD_GLOBAL);
  if (!library) {
    const char* library_error = dlerror();
    SetError(error, error_capacity, "librockchip_mpp.so is unavailable: %s",
             library_error ? library_error : "unknown error");
    return false;
  }

  static const char* const required_symbols[] = {
      "mpp_create",
      "mpp_init",
      "mpp_destroy",
      "mpp_check_support_format",
      "mpp_frame_init",
      "mpp_frame_deinit",
      "mpp_frame_set_width",
      "mpp_frame_set_height",
      "mpp_frame_set_hor_stride",
      "mpp_frame_set_ver_stride",
      "mpp_frame_set_fmt",
      "mpp_frame_set_buffer",
      "mpp_frame_get_width",
      "mpp_frame_get_height",
      "mpp_frame_get_hor_stride",
      "mpp_frame_get_ver_stride",
      "mpp_frame_get_fmt",
      "mpp_frame_get_buffer",
      "mpp_frame_get_errinfo",
      "mpp_frame_get_discard",
      "mpp_packet_init_with_buffer",
      "mpp_packet_deinit",
      "mpp_packet_set_length",
      "mpp_packet_get_meta",
      "mpp_meta_set_frame",
      "mpp_buffer_get_with_tag",
      "mpp_buffer_put_with_caller",
      "mpp_buffer_get_ptr_with_caller",
      "mpp_buffer_group_get",
      "mpp_buffer_group_put",
      "mpp_buffer_sync_begin_f",
      "mpp_buffer_sync_end_f",
  };

  for (const char* symbol : required_symbols) {
    dlerror();
    if (!dlsym(library, symbol)) {
      const char* symbol_error = dlerror();
      SetError(error, error_capacity,
               "librockchip_mpp.so does not provide %s: %s", symbol,
               symbol_error ? symbol_error : "unknown error");
      dlclose(library);
      return false;
    }
  }

  dlclose(library);
  return true;
}

// Once MPP accepts a packet, a failed retrieval can leave a delayed frame in
// its output queue. Flush that state before the next camera frame so callers
// never receive the previous JPEG for a newer timestamp.
class DecoderRecovery final {
 public:
  DecoderRecovery(MppCtx context, MppApi* api) : context_(context), api_(api) {}

  ~DecoderRecovery() {
    if (!armed_ || !context_ || !api_) {
      return;
    }
    api_->reset(context_);
    MppFrameFormat output_format = MPP_FMT_YUV420SP;
    api_->control(context_, MPP_DEC_SET_OUTPUT_FORMAT, &output_format);
  }

  void Arm() { armed_ = true; }
  void Disarm() { armed_ = false; }

 private:
  MppCtx context_ = nullptr;
  MppApi* api_ = nullptr;
  bool armed_ = false;
};

class MppMjpegDecoder final {
 public:
  MppMjpegDecoder(uint32_t width, uint32_t height)
      : width_(width), height_(height) {}

  ~MppMjpegDecoder() { Reset(); }

  MppMjpegDecoder(const MppMjpegDecoder&) = delete;
  MppMjpegDecoder& operator=(const MppMjpegDecoder&) = delete;

  bool Initialize(char* error, size_t error_capacity) {
    if (width_ == 0 || height_ == 0) {
      SetError(error, error_capacity, "MJPEG dimensions must be non-zero");
      return false;
    }
    if (!AlignUp(width_, &horizontal_stride_) ||
        !AlignUp(height_, &vertical_stride_)) {
      SetError(error, error_capacity, "MJPEG dimensions are too large");
      return false;
    }
    if (!CheckedOutputSize(horizontal_stride_, vertical_stride_,
                           &output_buffer_size_)) {
      SetError(error, error_capacity, "MJPEG output buffer size overflow");
      return false;
    }

    MPP_RET result =
        mpp_check_support_format(MPP_CTX_DEC, MPP_VIDEO_CodingMJPEG);
    if (result != MPP_OK) {
      SetError(error, error_capacity,
               "Rockchip MPP does not support MJPEG decoding (status %d)",
               result);
      return false;
    }

    result = mpp_create(&context_, &api_);
    if (result != MPP_OK || !context_ || !api_) {
      SetError(error, error_capacity, "mpp_create failed with status %d",
               result);
      Reset();
      return false;
    }

    result = mpp_init(context_, MPP_CTX_DEC, MPP_VIDEO_CodingMJPEG);
    if (result != MPP_OK) {
      SetError(error, error_capacity,
               "MPP MJPEG decoder initialization failed with status %d",
               result);
      Reset();
      return false;
    }

    MppFrameFormat output_format = MPP_FMT_YUV420SP;
    result = api_->control(context_, MPP_DEC_SET_OUTPUT_FORMAT, &output_format);
    if (result != MPP_OK) {
      SetError(error, error_capacity,
               "failed to request MPP NV12 decoder output (status %d)",
               result);
      Reset();
      return false;
    }

    result = mpp_buffer_group_get_internal(
        &buffer_group_, MPP_BUFFER_TYPE_DRM | MPP_BUFFER_FLAGS_CACHABLE);
    if (result != MPP_OK) {
      if (buffer_group_) {
        mpp_buffer_group_put(buffer_group_);
        buffer_group_ = nullptr;
      }
      result = mpp_buffer_group_get_internal(
          &buffer_group_, MPP_BUFFER_TYPE_ION | MPP_BUFFER_FLAGS_CACHABLE);
    }
    if (result != MPP_OK || !buffer_group_) {
      SetError(error, error_capacity,
               "failed to allocate an MPP decoder buffer group (status %d)",
               result);
      Reset();
      return false;
    }

    result =
        mpp_buffer_get(buffer_group_, &output_buffer_, output_buffer_size_);
    if (result != MPP_OK || !output_buffer_) {
      SetError(error, error_capacity,
               "failed to allocate the MPP decoder output buffer (status %d)",
               result);
      Reset();
      return false;
    }

    result = mpp_frame_init(&output_frame_);
    if (result != MPP_OK || !output_frame_) {
      SetError(error, error_capacity,
               "failed to allocate the MPP decoder output frame (status %d)",
               result);
      Reset();
      return false;
    }
    mpp_frame_set_width(output_frame_, width_);
    mpp_frame_set_height(output_frame_, height_);
    mpp_frame_set_hor_stride(output_frame_, horizontal_stride_);
    mpp_frame_set_ver_stride(output_frame_, vertical_stride_);
    mpp_frame_set_fmt(output_frame_, MPP_FMT_YUV420SP);
    mpp_frame_set_buffer(output_frame_, output_buffer_);
    return true;
  }

  int Decode(const uint8_t* source,
             size_t source_size,
             uint8_t* destination_y,
             size_t destination_y_size,
             uint32_t destination_stride_y,
             uint8_t* destination_uv,
             size_t destination_uv_size,
             uint32_t destination_stride_uv,
             char* error,
             size_t error_capacity) {
    ClearError(error, error_capacity);
    if (!context_ || !api_ || !output_frame_ || !source ||
        source_size == 0 || !destination_y || !destination_uv) {
      return SetError(error, error_capacity,
                      "invalid MPP MJPEG decoder state or frame buffers");
    }

    const uint32_t chroma_rows = (height_ + 1) / 2;
    const uint32_t chroma_row_bytes = ((width_ + 1) / 2) * 2;
    if (!CheckDestination(destination_y_size, destination_stride_y, width_,
                          height_) ||
        !CheckDestination(destination_uv_size, destination_stride_uv,
                          chroma_row_bytes, chroma_rows)) {
      return SetError(error, error_capacity,
                      "NV12 destination buffer is too small for %ux%u",
                      width_, height_);
    }

    if (!EnsureInputCapacity(source_size, error, error_capacity)) {
      return -1;
    }

    void* input = mpp_buffer_get_ptr(input_buffer_);
    if (!input) {
      return SetError(error, error_capacity,
                      "MPP MJPEG input buffer is not CPU-accessible");
    }
    MPP_RET result = mpp_buffer_sync_begin(input_buffer_);
    if (result != MPP_OK) {
      return SetError(error, error_capacity,
                      "failed to begin MPP input buffer access (status %d)",
                      result);
    }
    std::memcpy(input, source, source_size);
    result = mpp_buffer_sync_end(input_buffer_);
    if (result != MPP_OK) {
      return SetError(error, error_capacity,
                      "failed to finish MPP input buffer access (status %d)",
                      result);
    }

    MppPacket packet = nullptr;
    result = mpp_packet_init_with_buffer(&packet, input_buffer_);
    if (result != MPP_OK || !packet) {
      return SetError(error, error_capacity,
                      "mpp_packet_init_with_buffer failed with status %d",
                      result);
    }
    mpp_packet_set_length(packet, source_size);

    MppMeta metadata = mpp_packet_get_meta(packet);
    if (!metadata) {
      mpp_packet_deinit(&packet);
      return SetError(error, error_capacity,
                      "MPP MJPEG packet did not provide metadata");
    }
    result = mpp_meta_set_frame(metadata, KEY_OUTPUT_FRAME, output_frame_);
    if (result != MPP_OK) {
      mpp_packet_deinit(&packet);
      return SetError(error, error_capacity,
                      "failed to attach the MPP output frame (status %d)",
                      result);
    }

    DecoderRecovery recovery(context_, api_);
    for (int attempt = 0; attempt < kQueueAttempts; ++attempt) {
      result = api_->decode_put_packet(context_, packet);
      if (result == MPP_OK) {
        recovery.Arm();
        break;
      }
      if (result != MPP_NOK || attempt + 1 == kQueueAttempts) {
        mpp_packet_deinit(&packet);
        return SetError(error, error_capacity,
                        "MPP MJPEG packet submission failed with status %d",
                        result);
      }
      std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }

    MppFrame decoded_frame = nullptr;
    for (int attempt = 0; attempt < kFrameAttempts; ++attempt) {
      result = api_->decode_get_frame(context_, &decoded_frame);
      if (result == MPP_OK && decoded_frame) {
        break;
      }
      if (result != MPP_OK && result != MPP_ERR_TIMEOUT) {
        mpp_packet_deinit(&packet);
        return SetError(error, error_capacity,
                        "MPP MJPEG frame retrieval failed with status %d",
                        result);
      }
      std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
    mpp_packet_deinit(&packet);

    if (!decoded_frame) {
      return SetError(error, error_capacity,
                      "MPP MJPEG decoder produced no frame");
    }
    if (decoded_frame != output_frame_) {
      mpp_frame_deinit(&decoded_frame);
      return SetError(error, error_capacity,
                      "MPP MJPEG decoder returned an unexpected output frame");
    }

    const uint32_t decoded_width = mpp_frame_get_width(decoded_frame);
    const uint32_t decoded_height = mpp_frame_get_height(decoded_frame);
    const uint32_t decoded_horizontal_stride =
        mpp_frame_get_hor_stride(decoded_frame);
    const uint32_t decoded_vertical_stride =
        mpp_frame_get_ver_stride(decoded_frame);
    const MppFrameFormat decoded_format = mpp_frame_get_fmt(decoded_frame);
    if (decoded_width != width_ || decoded_height != height_) {
      return SetError(error, error_capacity,
                      "MPP decoded %ux%u instead of %ux%u", decoded_width,
                      decoded_height, width_, height_);
    }
    if ((decoded_format & MPP_FRAME_FMT_MASK) != MPP_FMT_YUV420SP) {
      return SetError(error, error_capacity,
                      "MPP returned pixel format 0x%x instead of NV12",
                      static_cast<unsigned int>(decoded_format));
    }
    if (decoded_horizontal_stride < width_ ||
        decoded_vertical_stride < height_) {
      return SetError(error, error_capacity,
                      "MPP returned invalid NV12 strides %ux%u",
                      decoded_horizontal_stride, decoded_vertical_stride);
    }
    if (mpp_frame_get_errinfo(decoded_frame) ||
        mpp_frame_get_discard(decoded_frame)) {
      return SetError(error, error_capacity,
                      "MPP marked the decoded MJPEG frame invalid");
    }

    MppBuffer decoded_buffer = mpp_frame_get_buffer(decoded_frame);
    if (!decoded_buffer) {
      return SetError(error, error_capacity,
                      "MPP MJPEG output frame has no buffer");
    }
    uint8_t* decoded =
        static_cast<uint8_t*>(mpp_buffer_get_ptr(decoded_buffer));
    if (!decoded) {
      return SetError(error, error_capacity,
                      "MPP MJPEG output buffer is not CPU-accessible");
    }
    result = mpp_buffer_sync_begin(decoded_buffer);
    if (result != MPP_OK) {
      return SetError(error, error_capacity,
                      "failed to begin MPP output buffer access (status %d)",
                      result);
    }

    const uint8_t* source_y = decoded;
    const uint8_t* source_uv =
        decoded + static_cast<size_t>(decoded_horizontal_stride) *
                      decoded_vertical_stride;
    for (uint32_t row = 0; row < height_; ++row) {
      std::memcpy(destination_y + static_cast<size_t>(row) *
                                      destination_stride_y,
                  source_y + static_cast<size_t>(row) *
                                 decoded_horizontal_stride,
                  width_);
    }
    for (uint32_t row = 0; row < chroma_rows; ++row) {
      std::memcpy(destination_uv + static_cast<size_t>(row) *
                                       destination_stride_uv,
                  source_uv + static_cast<size_t>(row) *
                                  decoded_horizontal_stride,
                  chroma_row_bytes);
    }

    result = mpp_buffer_sync_end(decoded_buffer);
    if (result != MPP_OK) {
      return SetError(error, error_capacity,
                      "failed to finish MPP output buffer access (status %d)",
                      result);
    }
    recovery.Disarm();
    return 0;
  }

 private:
  bool EnsureInputCapacity(size_t required,
                           char* error,
                           size_t error_capacity) {
    if (input_buffer_ && input_capacity_ >= required) {
      return true;
    }
    if (input_buffer_) {
      mpp_buffer_put(input_buffer_);
      input_buffer_ = nullptr;
      input_capacity_ = 0;
    }

    const size_t capacity = required < 4096 ? 4096 : required;
    const MPP_RET result =
        mpp_buffer_get(buffer_group_, &input_buffer_, capacity);
    if (result != MPP_OK || !input_buffer_) {
      SetError(error, error_capacity,
               "failed to allocate an MPP MJPEG input buffer (status %d)",
               result);
      return false;
    }
    input_capacity_ = capacity;
    return true;
  }

  void Reset() {
    if (context_ && api_) {
      api_->reset(context_);
    }
    if (output_frame_) {
      mpp_frame_deinit(&output_frame_);
      output_frame_ = nullptr;
    }
    if (context_) {
      mpp_destroy(context_);
      context_ = nullptr;
      api_ = nullptr;
    }
    if (input_buffer_) {
      mpp_buffer_put(input_buffer_);
      input_buffer_ = nullptr;
      input_capacity_ = 0;
    }
    if (output_buffer_) {
      mpp_buffer_put(output_buffer_);
      output_buffer_ = nullptr;
    }
    if (buffer_group_) {
      mpp_buffer_group_put(buffer_group_);
      buffer_group_ = nullptr;
    }
  }

  uint32_t width_ = 0;
  uint32_t height_ = 0;
  uint32_t horizontal_stride_ = 0;
  uint32_t vertical_stride_ = 0;
  size_t output_buffer_size_ = 0;
  size_t input_capacity_ = 0;
  MppCtx context_ = nullptr;
  MppApi* api_ = nullptr;
  MppBufferGroup buffer_group_ = nullptr;
  MppBuffer input_buffer_ = nullptr;
  MppBuffer output_buffer_ = nullptr;
  MppFrame output_frame_ = nullptr;
};

}  // namespace

struct LkMppMjpegDecoder {
  explicit LkMppMjpegDecoder(uint32_t width, uint32_t height)
      : decoder(width, height) {}

  MppMjpegDecoder decoder;
};

extern "C" LkMppMjpegDecoder* lk_mpp_mjpeg_decoder_create(
    uint32_t width,
    uint32_t height,
    char* error,
    size_t error_capacity) {
  ClearError(error, error_capacity);
  if (!HasRequiredMppSymbols(error, error_capacity)) {
    return nullptr;
  }

  LkMppMjpegDecoder* decoder =
      new (std::nothrow) LkMppMjpegDecoder(width, height);
  if (!decoder) {
    SetError(error, error_capacity,
             "failed to allocate the Rockchip MPP MJPEG decoder");
    return nullptr;
  }
  if (!decoder->decoder.Initialize(error, error_capacity)) {
    delete decoder;
    return nullptr;
  }
  return decoder;
}

extern "C" int lk_mpp_mjpeg_decoder_decode(
    LkMppMjpegDecoder* decoder,
    const uint8_t* source,
    size_t source_size,
    uint8_t* destination_y,
    size_t destination_y_size,
    uint32_t destination_stride_y,
    uint8_t* destination_uv,
    size_t destination_uv_size,
    uint32_t destination_stride_uv,
    char* error,
    size_t error_capacity) {
  if (!decoder) {
    return SetError(error, error_capacity,
                    "Rockchip MPP MJPEG decoder is null");
  }
  return decoder->decoder.Decode(
      source, source_size, destination_y, destination_y_size,
      destination_stride_y, destination_uv, destination_uv_size,
      destination_stride_uv, error, error_capacity);
}

extern "C" void lk_mpp_mjpeg_decoder_destroy(
    LkMppMjpegDecoder* decoder) {
  delete decoder;
}
