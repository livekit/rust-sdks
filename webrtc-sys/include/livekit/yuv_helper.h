//
// Created by Théo Monnom on 01/12/2022.
//

#pragma once

#include <memory>

#include "api/video/yuv_helper.h"
#include "webrtc-sys/src/yuv_helper.rs.h"

namespace livekit {

static void i420_to_argb(const uint8_t* src_y,
                         int src_stride_y,
                         const uint8_t* src_u,
                         int src_stride_u,
                         const uint8_t* src_v,
                         int src_stride_v,
                         uint8_t* dst_argb,
                         int dst_stride_argb,
                         int width,
                         int height) {
  webrtc::I420ToARGB(src_y, src_stride_y, src_u, src_stride_u, src_v,
                     src_stride_v, dst_argb, dst_stride_argb, width, height);
}

static void i420_to_bgra(const uint8_t* src_y,
                         int src_stride_y,
                         const uint8_t* src_u,
                         int src_stride_u,
                         const uint8_t* src_v,
                         int src_stride_v,
                         uint8_t* dst_bgra,
                         int dst_stride_bgra,
                         int width,
                         int height) {
  webrtc::I420ToBGRA(src_y, src_stride_y, src_u, src_stride_u, src_v,
                     src_stride_v, dst_bgra, dst_stride_bgra, width, height);
}

static void i420_to_abgr(const uint8_t* src_y,
                         int src_stride_y,
                         const uint8_t* src_u,
                         int src_stride_u,
                         const uint8_t* src_v,
                         int src_stride_v,
                         uint8_t* dst_abgr,
                         int dst_stride_abgr,
                         int width,
                         int height) {
  webrtc::I420ToABGR(src_y, src_stride_y, src_u, src_stride_u, src_v,
                     src_stride_v, dst_abgr, dst_stride_abgr, width, height);
}

static void i420_to_rgba(const uint8_t* src_y,
                         int src_stride_y,
                         const uint8_t* src_u,
                         int src_stride_u,
                         const uint8_t* src_v,
                         int src_stride_v,
                         uint8_t* dst_rgba,
                         int dst_stride_rgba,
                         int width,
                         int height) {
  webrtc::I420ToRGBA(src_y, src_stride_y, src_u, src_stride_u, src_v,
                     src_stride_v, dst_rgba, dst_stride_rgba, width, height);
}

}  // namespace livekit
