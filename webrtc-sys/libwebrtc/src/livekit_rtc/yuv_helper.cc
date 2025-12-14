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

#include "api/video/yuv_helper.h"
#include "livekit_rtc/include/yuv_helper.h"

#include <iostream>
#include <memory>
#include <stdexcept>
#include <string>

#define THROW_ON_ERROR(ret)                                            \
  if (ret != 0) {                                                      \
    std::cout << "libyuv error: " << std::to_string(ret) << std::endl; \
  }

void lkI420ToARGB(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_u,
                  int src_stride_u,
                  const uint8_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_argb,
                  int dst_stride_argb,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I420ToARGB(src_y, src_stride_y, src_u, src_stride_u,
                                    src_v, src_stride_v, dst_argb,
                                    dst_stride_argb, width, height));
}

void lkI420ToBGRA(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_u,
                  int src_stride_u,
                  const uint8_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_bgra,
                  int dst_stride_bgra,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I420ToBGRA(src_y, src_stride_y, src_u, src_stride_u,
                                    src_v, src_stride_v, dst_bgra,
                                    dst_stride_bgra, width, height));
}

void lkI420ToABGR(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_u,
                  int src_stride_u,
                  const uint8_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_abgr,
                  int dst_stride_abgr,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I420ToABGR(src_y, src_stride_y, src_u, src_stride_u,
                                    src_v, src_stride_v, dst_abgr,
                                    dst_stride_abgr, width, height));
}

void lkI420ToRGBA(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_u,
                  int src_stride_u,
                  const uint8_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_rgba,
                  int dst_stride_rgba,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I420ToRGBA(src_y, src_stride_y, src_u, src_stride_u,
                                    src_v, src_stride_v, dst_rgba,
                                    dst_stride_rgba, width, height));
}

void lkARGBToI420(const uint8_t* src_argb,
                  int src_stride_argb,
                  uint8_t* dst_y,
                  int dst_stride_y,
                  uint8_t* dst_u,
                  int dst_stride_u,
                  uint8_t* dst_v,
                  int dst_stride_v,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::ARGBToI420(src_argb, src_stride_argb, dst_y,
                                    dst_stride_y, dst_u, dst_stride_u, dst_v,
                                    dst_stride_v, width, height));
}

void lkABGRToI420(const uint8_t* src_abgr,
                  int src_stride_abgr,
                  uint8_t* dst_y,
                  int dst_stride_y,
                  uint8_t* dst_u,
                  int dst_stride_u,
                  uint8_t* dst_v,
                  int dst_stride_v,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::ABGRToI420(src_abgr, src_stride_abgr, dst_y,
                                    dst_stride_y, dst_u, dst_stride_u, dst_v,
                                    dst_stride_v, width, height));
}

void lkARGBToRGB24(const uint8_t* src_argb,
                   int src_stride_argb,
                   uint8_t* dst_rgb24,
                   int dst_stride_rgb24,
                   int width,
                   int height) {
  THROW_ON_ERROR(webrtc::ARGBToRGB24(src_argb, src_stride_argb, dst_rgb24,
                                     dst_stride_rgb24, width, height));
}

void lkI420ToNV12(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_u,
                  int src_stride_u,
                  const uint8_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_y,
                  int dst_stride_y,
                  uint8_t* dst_uv,
                  int dst_stride_uv,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I420ToNV12(src_y, src_stride_y, src_u, src_stride_u,
                                    src_v, src_stride_v, dst_y, dst_stride_y,
                                    dst_uv, dst_stride_uv, width, height));
}

void lkNV12ToI420(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_uv,
                  int src_stride_uv,
                  uint8_t* dst_y,
                  int dst_stride_y,
                  uint8_t* dst_u,
                  int dst_stride_u,
                  uint8_t* dst_v,
                  int dst_stride_v,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::NV12ToI420(src_y, src_stride_y, src_uv, src_stride_uv,
                                    dst_y, dst_stride_y, dst_u, dst_stride_u,
                                    dst_v, dst_stride_v, width, height));
}

void lkI420ToNV12(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_u,
                  int src_stride_u,
                  uint8_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_y,
                  int dst_stride_y,
                  uint8_t* dst_uv,
                  int dst_stride_uv,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::NV12ToI420(src_y, src_stride_y, src_u, src_stride_u,
                                    src_v, src_stride_v, dst_y, dst_stride_y,
                                    dst_uv, dst_stride_uv, width, height));
}

void lkI444ToI420(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_u,
                  int src_stride_u,
                  const uint8_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_y,
                  int dst_stride_y,
                  uint8_t* dst_u,
                  int dst_stride_u,
                  uint8_t* dst_v,
                  int dst_stride_v,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I444ToI420(
      src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, dst_y,
      dst_stride_y, dst_u, dst_stride_u, dst_v, dst_stride_v, width, height));
}

void lkI422ToI420(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_u,
                  int src_stride_u,
                  const uint8_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_y,
                  int dst_stride_y,
                  uint8_t* dst_u,
                  int dst_stride_u,
                  uint8_t* dst_v,
                  int dst_stride_v,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I422ToI420(
      src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, dst_y,
      dst_stride_y, dst_u, dst_stride_u, dst_v, dst_stride_v, width, height));
}

void lkI010ToI420(const uint16_t* src_y,
                  int src_stride_y,
                  const uint16_t* src_u,
                  int src_stride_u,
                  const uint16_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_y,
                  int dst_stride_y,
                  uint8_t* dst_u,
                  int dst_stride_u,
                  uint8_t* dst_v,
                  int dst_stride_v,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I010ToI420(
      src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, dst_y,
      dst_stride_y, dst_u, dst_stride_u, dst_v, dst_stride_v, width, height));
}

void lkNV12ToARGB(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_uv,
                  int src_stride_uv,
                  uint8_t* dst_argb,
                  int dst_stride_argb,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::NV12ToARGB(src_y, src_stride_y, src_uv, src_stride_uv,
                                    dst_argb, dst_stride_argb, width, height));
}

void lkNV12ToABGR(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_uv,
                  int src_stride_uv,
                  uint8_t* dst_abgr,
                  int dst_stride_abgr,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::NV12ToABGR(src_y, src_stride_y, src_uv, src_stride_uv,
                                    dst_abgr, dst_stride_abgr, width, height));
}

void lkI444ToARGB(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_u,
                  int src_stride_u,
                  const uint8_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_abgr,
                  int dst_stride_abgr,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I444ToARGB(src_y, src_stride_y, src_u, src_stride_u,
                                    src_v, src_stride_v, dst_abgr,
                                    dst_stride_abgr, width, height));
}

void lkI444ToABGR(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_u,
                  int src_stride_u,
                  const uint8_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_abgr,
                  int dst_stride_abgr,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I444ToABGR(src_y, src_stride_y, src_u, src_stride_u,
                                    src_v, src_stride_v, dst_abgr,
                                    dst_stride_abgr, width, height));
}

void lkI422ToARGB(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_u,
                  int src_stride_u,
                  const uint8_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_argb,
                  int dst_stride_argb,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I422ToARGB(src_y, src_stride_y, src_u, src_stride_u,
                                    src_v, src_stride_v, dst_argb,
                                    dst_stride_argb, width, height));
}

void lk422ToABGR(const uint8_t* src_y,
                  int src_stride_y,
                  const uint8_t* src_u,
                  int src_stride_u,
                  const uint8_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_abgr,
                  int dst_stride_abgr,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I422ToABGR(src_y, src_stride_y, src_u, src_stride_u,
                                    src_v, src_stride_v, dst_abgr,
                                    dst_stride_abgr, width, height));
}

void lkI010ToARGB(const uint16_t* src_y,
                  int src_stride_y,
                  const uint16_t* src_u,
                  int src_stride_u,
                  const uint16_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_argb,
                  int dst_stride_argb,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I010ToARGB(src_y, src_stride_y, src_u, src_stride_u,
                                    src_v, src_stride_v, dst_argb,
                                    dst_stride_argb, width, height));
}

void lkI010ToABGR(const uint16_t* src_y,
                  int src_stride_y,
                  const uint16_t* src_u,
                  int src_stride_u,
                  const uint16_t* src_v,
                  int src_stride_v,
                  uint8_t* dst_abgr,
                  int dst_stride_abgr,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::I010ToABGR(src_y, src_stride_y, src_u, src_stride_u,
                                    src_v, src_stride_v, dst_abgr,
                                    dst_stride_abgr, width, height));
}

void lkABGRToNV12(const uint8_t* src_abgr,
                  int src_stride_abgr,
                  uint8_t* dst_y,
                  int dst_stride_y,
                  uint8_t* dst_uv,
                  int dst_stride_uv,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::ABGRToNV12(src_abgr, src_stride_abgr, dst_y,
                                    dst_stride_y, dst_uv, dst_stride_uv, width,
                                    height));
}

void lkARGBToNV12(const uint8_t* src_argb,
                  int src_stride_argb,
                  uint8_t* dst_y,
                  int dst_stride_y,
                  uint8_t* dst_uv,
                  int dst_stride_uv,
                  int width,
                  int height) {
  THROW_ON_ERROR(webrtc::ARGBToNV12(src_argb, src_stride_argb, dst_y,
                                    dst_stride_y, dst_uv, dst_stride_uv, width,
                                    height));
}
