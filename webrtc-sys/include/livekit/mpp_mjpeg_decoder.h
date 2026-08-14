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

#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct LkMppMjpegDecoder LkMppMjpegDecoder;

// Creates a persistent Rockchip MPP MJPEG decoder for the requested dimensions.
// Returns null and writes a diagnostic to error when hardware decoding is unavailable.
LkMppMjpegDecoder* lk_mpp_mjpeg_decoder_create(uint32_t width,
                                                uint32_t height,
                                                char* error,
                                                size_t error_capacity);

// Decodes one complete JPEG image into visible NV12 planes.
// Returns zero on success and writes a diagnostic to error on failure.
int lk_mpp_mjpeg_decoder_decode(LkMppMjpegDecoder* decoder,
                                const uint8_t* source,
                                size_t source_size,
                                uint8_t* destination_y,
                                size_t destination_y_size,
                                uint32_t destination_stride_y,
                                uint8_t* destination_uv,
                                size_t destination_uv_size,
                                uint32_t destination_stride_uv,
                                char* error,
                                size_t error_capacity);

// Releases a decoder created by lk_mpp_mjpeg_decoder_create.
void lk_mpp_mjpeg_decoder_destroy(LkMppMjpegDecoder* decoder);

#ifdef __cplusplus
}  // extern "C"
#endif
