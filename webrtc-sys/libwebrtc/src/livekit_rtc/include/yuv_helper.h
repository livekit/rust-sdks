#ifndef LIVEKIT_YUV_HELPER_CAPI_H
#define LIVEKIT_YUV_HELPER_CAPI_H

#include <stdbool.h>
#include <stdint.h>

#define LK_EXPORT __attribute__((visibility("default")))

#ifdef __cplusplus
extern "C" {
#endif

LK_EXPORT void lkI420ToARGB(const uint8_t* src_y,
                            int src_stride_y,
                            const uint8_t* src_u,
                            int src_stride_u,
                            const uint8_t* src_v,
                            int src_stride_v,
                            uint8_t* dst_argb,
                            int dst_stride_argb,
                            int width,
                            int height);

LK_EXPORT void lkI420ToBGRA(const uint8_t* src_y,
                            int src_stride_y,
                            const uint8_t* src_u,
                            int src_stride_u,
                            const uint8_t* src_v,
                            int src_stride_v,
                            uint8_t* dst_bgra,
                            int dst_stride_bgra,
                            int width,
                            int height);

LK_EXPORT void lkI420ToABGR(const uint8_t* src_y,
                            int src_stride_y,
                            const uint8_t* src_u,
                            int src_stride_u,
                            const uint8_t* src_v,
                            int src_stride_v,
                            uint8_t* dst_abgr,
                            int dst_stride_abgr,
                            int width,
                            int height);

LK_EXPORT void lkI420ToRGBA(const uint8_t* src_y,
                            int src_stride_y,
                            const uint8_t* src_u,
                            int src_stride_u,
                            const uint8_t* src_v,
                            int src_stride_v,
                            uint8_t* dst_rgba,
                            int dst_stride_rgba,
                            int width,
                            int height);

LK_EXPORT void lkARGBToI420(const uint8_t* src_argb,
                            int src_stride_argb,
                            uint8_t* dst_y,
                            int dst_stride_y,
                            uint8_t* dst_u,
                            int dst_stride_u,
                            uint8_t* dst_v,
                            int dst_stride_v,
                            int width,
                            int height);

LK_EXPORT void lkABGRToI420(const uint8_t* src_abgr,
                            int src_stride_abgr,
                            uint8_t* dst_y,
                            int dst_stride_y,
                            uint8_t* dst_u,
                            int dst_stride_u,
                            uint8_t* dst_v,
                            int dst_stride_v,
                            int width,
                            int height);

LK_EXPORT void lkARGBToRGB24(const uint8_t* src_argb,
                             int src_stride_argb,
                             uint8_t* dst_rgb24,
                             int dst_stride_rgb24,
                             int width,
                             int height);

LK_EXPORT void lkI420ToNV12(const uint8_t* src_y,
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
                            int height);

LK_EXPORT void lkNV12ToI420(const uint8_t* src_y,
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
                            int height);

LK_EXPORT void lkI444ToI420(const uint8_t* src_y,
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
                            int height);

LK_EXPORT void lkI422ToI420(const uint8_t* src_y,
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
                            int height);

LK_EXPORT void lkI010ToI420(const uint16_t* src_y,
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
                            int height);

LK_EXPORT void lkNV12ToARGB(const uint8_t* src_y,
                            int src_stride_y,
                            const uint8_t* src_uv,
                            int src_stride_uv,
                            uint8_t* dst_argb,
                            int dst_stride_argb,
                            int width,
                            int height);

LK_EXPORT void lkNV12ToABGR(const uint8_t* src_y,
                            int src_stride_y,
                            const uint8_t* src_uv,
                            int src_stride_uv,
                            uint8_t* dst_abgr,
                            int dst_stride_abgr,
                            int width,
                            int height);

LK_EXPORT void lkI444ToARGB(const uint8_t* src_y,
                            int src_stride_y,
                            const uint8_t* src_u,
                            int src_stride_u,
                            const uint8_t* src_v,
                            int src_stride_v,
                            uint8_t* dst_abgr,
                            int dst_stride_abgr,
                            int width,
                            int height);

LK_EXPORT void lkI444ToABGR(const uint8_t* src_y,
                            int src_stride_y,
                            const uint8_t* src_u,
                            int src_stride_u,
                            const uint8_t* src_v,
                            int src_stride_v,
                            uint8_t* dst_abgr,
                            int dst_stride_abgr,
                            int width,
                            int height);

LK_EXPORT void lkI422ToARGB(const uint8_t* src_y,
                            int src_stride_y,
                            const uint8_t* src_u,
                            int src_stride_u,
                            const uint8_t* src_v,
                            int src_stride_v,
                            uint8_t* dst_argb,
                            int dst_stride_argb,
                            int width,
                            int height);

LK_EXPORT void lk422ToABGR(const uint8_t* src_y,
                           int src_stride_y,
                           const uint8_t* src_u,
                           int src_stride_u,
                           const uint8_t* src_v,
                           int src_stride_v,
                           uint8_t* dst_abgr,
                           int dst_stride_abgr,
                           int width,
                           int height);

LK_EXPORT void lkI010ToARGB(const uint16_t* src_y,
                            int src_stride_y,
                            const uint16_t* src_u,
                            int src_stride_u,
                            const uint16_t* src_v,
                            int src_stride_v,
                            uint8_t* dst_argb,
                            int dst_stride_argb,
                            int width,
                            int height);

LK_EXPORT void lkI010ToABGR(const uint16_t* src_y,
                            int src_stride_y,
                            const uint16_t* src_u,
                            int src_stride_u,
                            const uint16_t* src_v,
                            int src_stride_v,
                            uint8_t* dst_abgr,
                            int dst_stride_abgr,
                            int width,
                            int height);

LK_EXPORT void lkABGRToNV12(const uint8_t* src_abgr,
                            int src_stride_abgr,
                            uint8_t* dst_y,
                            int dst_stride_y,
                            uint8_t* dst_uv,
                            int dst_stride_uv,
                            int width,
                            int height);

LK_EXPORT void lkARGBToNV12(const uint8_t* src_argb,
                            int src_stride_argb,
                            uint8_t* dst_y,
                            int dst_stride_y,
                            uint8_t* dst_uv,
                            int dst_stride_uv,
                            int width,
                            int height);

#ifdef __cplusplus
}
#endif

#endif  // LIVEKIT_YUV_HELPER_CAPI_H
