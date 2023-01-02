//
// Created by Théo Monnom on 01/12/2022.
//

#ifndef CLIENT_SDK_NATIVE_YUV_HELPER_H
#define CLIENT_SDK_NATIVE_YUV_HELPER_H

#include <memory>

#include "api/video/yuv_helper.h"

namespace livekit {

static void i420_to_abgr(const uint8_t* src_y,
                       int src_stride_y,
                       const uint8_t* src_u,
                       int src_stride_u,
                       const uint8_t* src_v,
                       int src_stride_v,
                       uint8_t* dst_rgba,
                       int dst_stride_abgr,
                       int width,
                       int height) {
  webrtc::I420ToABGR(src_y, src_stride_y, src_u, src_stride_u, src_v,
                     src_stride_v, dst_rgba, dst_stride_abgr, width, height);
}

}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_YUV_HELPER_H
