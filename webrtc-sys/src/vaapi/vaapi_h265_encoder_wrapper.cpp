#include "vaapi_h265_encoder_wrapper.h"

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <algorithm>
#include <memory>
#include <vector>

#include "rtc_base/logging.h"

#define VA_FOURCC_I420 0x30323449

static const uint32_t kCtuSize = 32;

static const int kH265FrameP = 0;
static const int kH265FrameI = 2;
static const int kH265FrameIdr = 7;

static uint32_t align_up(uint32_t value, uint32_t alignment) {
  return (value + alignment - 1) & ~(alignment - 1);
}

static bool va_feature_enabled(uint32_t feature) {
  return feature != 0;
}

static void destroy_buffers(VADisplay va_dpy,
                            const std::vector<VABufferID>& buffers) {
  for (VABufferID buffer : buffers) {
    if (buffer == VA_INVALID_ID) {
      continue;
    }
    VAStatus va_status = vaDestroyBuffer(va_dpy, buffer);
    if (va_status != VA_STATUS_SUCCESS) {
      RTC_LOG(LS_WARNING) << "vaDestroyBuffer failed va_status = "
                          << va_status;
    }
  }
}

static bool has_start_code(const uint8_t* data, size_t size) {
  return (size >= 3 && data[0] == 0x00 && data[1] == 0x00 &&
          data[2] == 0x01) ||
         (size >= 4 && data[0] == 0x00 && data[1] == 0x00 &&
          data[2] == 0x00 && data[3] == 0x01);
}

static bool contains_start_code(const std::vector<uint8_t>& data) {
  for (size_t i = 0; i < data.size(); i++) {
    if (has_start_code(data.data() + i, data.size() - i)) {
      return true;
    }
  }
  return false;
}

static bool is_h265_nal_header(const uint8_t* data, size_t size) {
  if (size < 2) {
    return false;
  }

  const uint8_t forbidden_zero_bit = data[0] & 0x80;
  const uint8_t nal_unit_type = (data[0] & 0x7e) >> 1;
  const uint8_t nuh_temporal_id_plus1 = data[1] & 0x07;
  return forbidden_zero_bit == 0 && nal_unit_type <= 40 &&
         nuh_temporal_id_plus1 != 0;
}

static uint32_t read_be_length(const uint8_t* data, size_t length_size) {
  uint32_t length = 0;
  for (size_t i = 0; i < length_size; i++) {
    length = (length << 8) | data[i];
  }
  return length;
}

static bool append_length_prefixed_segments(const uint8_t* data,
                                            size_t size,
                                            size_t length_size,
                                            std::vector<uint8_t>* encoded) {
  static const uint8_t kStartCode[] = {0x00, 0x00, 0x00, 0x01};
  size_t offset = 0;
  std::vector<uint8_t> converted;

  while (offset < size) {
    if (size - offset < length_size) {
      return false;
    }

    uint32_t nalu_size = read_be_length(data + offset, length_size);
    offset += length_size;
    if (nalu_size == 0 || nalu_size > size - offset ||
        !is_h265_nal_header(data + offset, nalu_size)) {
      return false;
    }

    converted.insert(converted.end(), kStartCode,
                     kStartCode + sizeof(kStartCode));
    converted.insert(converted.end(), data + offset, data + offset + nalu_size);
    offset += nalu_size;
  }

  encoded->insert(encoded->end(), converted.begin(), converted.end());
  return true;
}

static void append_annex_b_segment(const uint8_t* data,
                                   size_t size,
                                   std::vector<uint8_t>* encoded) {
  static const uint8_t kStartCode[] = {0x00, 0x00, 0x00, 0x01};
  if (size == 0) {
    return;
  }

  if (has_start_code(data, size)) {
    encoded->insert(encoded->end(), data, data + size);
    return;
  }

  if ((append_length_prefixed_segments(data, size, 4, encoded)) ||
      (append_length_prefixed_segments(data, size, 2, encoded))) {
    return;
  }

  if (is_h265_nal_header(data, size)) {
    encoded->insert(encoded->end(), kStartCode,
                    kStartCode + sizeof(kStartCode));
  }
  encoded->insert(encoded->end(), data, data + size);
}

static int upload_surface_yuv(VADisplay va_dpy,
                              VASurfaceID surface_id,
                              int src_fourcc,
                              int src_width,
                              int src_height,
                              const uint8_t* src_Y,
                              int src_stride_Y,
                              const uint8_t* src_U,
                              int src_stride_U,
                              const uint8_t* src_V,
                              int src_stride_V) {
  VAImageFormat kImageFormatI420 = {
      .fourcc = VA_FOURCC_I420,
      .byte_order = VA_LSB_FIRST,
      .bits_per_pixel = 12,
  };

  VAImage surface_image;
  uint8_t *surface_p = NULL, *Y_start = NULL, *U_start = NULL,
          *V_start = NULL;
  int Y_pitch = 0, U_pitch = 0, V_pitch = 0, row;
  VAStatus va_status;
  bool derived_image = true;

  va_status = vaDeriveImage(va_dpy, surface_id, &surface_image);
  if (va_status != VA_STATUS_SUCCESS) {
    derived_image = false;
    va_status = vaCreateImage(va_dpy, &kImageFormatI420, src_width, src_height,
                              &surface_image);
    if (va_status != VA_STATUS_SUCCESS) {
      RTC_LOG(LS_ERROR) << "vaCreateImage failed with status " << va_status;
      return -1;
    }
  }

  va_status = vaMapBuffer(va_dpy, surface_image.buf, (void**)&surface_p);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaMapBuffer failed with status " << va_status;
    vaDestroyImage(va_dpy, surface_image.image_id);
    return -1;
  }

  Y_start = surface_p;
  Y_pitch = surface_image.pitches[0];
  switch (surface_image.format.fourcc) {
    case VA_FOURCC_NV12: {
      U_start = surface_p + surface_image.offsets[1];
      U_pitch = surface_image.pitches[1];
      break;
    }
    case VA_FOURCC_I420:
      U_start = surface_p + surface_image.offsets[1];
      U_pitch = surface_image.pitches[1];
      V_start = surface_p + surface_image.offsets[2];
      V_pitch = surface_image.pitches[2];
      break;
    case VA_FOURCC_YV12:
      U_start = surface_p + surface_image.offsets[2];
      U_pitch = surface_image.pitches[2];
      V_start = surface_p + surface_image.offsets[1];
      V_pitch = surface_image.pitches[1];
      break;
    default:
      RTC_LOG(LS_ERROR) << "Unsupported VA surface image format: "
                        << surface_image.format.fourcc;
      vaUnmapBuffer(va_dpy, surface_image.buf);
      vaDestroyImage(va_dpy, surface_image.image_id);
      return -1;
  }

  for (row = 0; row < src_height; row++) {
    memcpy(Y_start + row * Y_pitch, src_Y + row * src_stride_Y, src_width);
  }

  if (surface_image.format.fourcc == VA_FOURCC_NV12) {
    for (row = 0; row < src_height / 2; row++) {
      uint8_t* UV_row = U_start + row * U_pitch;
      if (src_fourcc == VA_FOURCC_NV12) {
        memcpy(UV_row, src_U + row * src_stride_U, src_width);
        continue;
      }

      const uint8_t* u_ptr = src_U + row * src_stride_U;
      const uint8_t* v_ptr = src_V + row * src_stride_V;
      if (src_fourcc == VA_FOURCC_YV12) {
        std::swap(u_ptr, v_ptr);
      }
      for (int col = 0; col < src_width / 2; col++) {
        UV_row[2 * col] = u_ptr[col];
        UV_row[2 * col + 1] = v_ptr[col];
      }
    }
  } else {
    for (row = 0; row < src_height / 2; row++) {
      if (src_fourcc == VA_FOURCC_NV12) {
        const uint8_t* uv_ptr = src_U + row * src_stride_U;
        uint8_t* U_row = U_start + row * U_pitch;
        uint8_t* V_row = V_start + row * V_pitch;
        for (int col = 0; col < src_width / 2; col++) {
          U_row[col] = uv_ptr[2 * col];
          V_row[col] = uv_ptr[2 * col + 1];
        }
        continue;
      }

      const uint8_t* u_ptr = src_U + row * src_stride_U;
      const uint8_t* v_ptr = src_V + row * src_stride_V;
      if (src_fourcc == VA_FOURCC_YV12) {
        std::swap(u_ptr, v_ptr);
      }
      memcpy(U_start + row * U_pitch, u_ptr, src_width / 2);
      memcpy(V_start + row * V_pitch, v_ptr, src_width / 2);
    }
  }

  vaUnmapBuffer(va_dpy, surface_image.buf);
  if (!derived_image) {
    va_status =
        vaPutImage(va_dpy, surface_id, surface_image.image_id, 0, 0, src_width,
                   src_height, 0, 0, src_width, src_height);
    if (va_status != VA_STATUS_SUCCESS) {
      RTC_LOG(LS_ERROR) << "vaPutImage failed with status " << va_status;
      vaDestroyImage(va_dpy, surface_image.image_id);
      return -1;
    }
  }
  vaDestroyImage(va_dpy, surface_image.image_id);
  return 0;
}

static void h265_encoding2display_order(uint64_t encoding_order,
                                        int intra_period,
                                        int intra_idr_period,
                                        uint64_t* displaying_order,
                                        int* frame_type) {
  *displaying_order = encoding_order;
  if (encoding_order == 0 ||
      (intra_idr_period > 0 && encoding_order % intra_idr_period == 0)) {
    *frame_type = kH265FrameIdr;
  } else if (intra_period > 0 && encoding_order % intra_period == 0) {
    *frame_type = kH265FrameI;
  } else {
    *frame_type = kH265FrameP;
  }
}

static void init_invalid_picture(VAPictureHEVC* pic) {
  pic->picture_id = VA_INVALID_SURFACE;
  pic->pic_order_cnt = 0;
  pic->flags = VA_PICTURE_HEVC_INVALID;
}

static bool select_entrypoint(VA265Context* context) {
  int num_entrypoints = vaMaxNumEntrypoints(context->va_dpy);
  auto entrypoints = std::make_unique<VAEntrypoint[]>(num_entrypoints);
  if (!entrypoints) {
    RTC_LOG(LS_ERROR) << "failed to allocate VA entrypoints";
    return false;
  }

  vaQueryConfigEntrypoints(context->va_dpy, context->config.h265_profile,
                           entrypoints.get(), &num_entrypoints);
  for (int i = 0; i < num_entrypoints; i++) {
    if (entrypoints[i] == VAEntrypointEncSliceLP ||
        entrypoints[i] == VAEntrypointEncSlice) {
      context->selected_entrypoint = entrypoints[i];
      RTC_LOG(LS_INFO) << "Using HEVC VAAPI EntryPoint - "
                       << context->selected_entrypoint;
      return true;
    }
  }

  RTC_LOG(LS_ERROR)
      << "Can't find VAEntrypointEncSlice or VAEntrypointEncSliceLP for HEVC";
  return false;
}

static VAStatus init_va(VA265Context* context, VADisplay va_dpy) {
  int major_ver, minor_ver;
  context->va_dpy = va_dpy;
  if (!context->va_dpy) {
    return VA_STATUS_ERROR_INVALID_DISPLAY;
  }

  VAStatus va_status = vaInitialize(context->va_dpy, &major_ver, &minor_ver);
  if (major_ver < 0 || minor_ver < 0 || va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaInitialize failed";
    return VA_STATUS_ERROR_INVALID_DISPLAY;
  }

  if (!select_entrypoint(context)) {
    return VA_STATUS_ERROR_UNSUPPORTED_ENTRYPOINT;
  }

  for (int i = 0; i < VAConfigAttribTypeMax; i++) {
    context->attrib[i].type = (VAConfigAttribType)i;
  }

  va_status = vaGetConfigAttributes(
      context->va_dpy, context->config.h265_profile,
      (VAEntrypoint)context->selected_entrypoint, &context->attrib[0],
      VAConfigAttribTypeMax);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaGetConfigAttributes failed";
    return va_status;
  }

  if ((context->attrib[VAConfigAttribRTFormat].value & VA_RT_FORMAT_YUV420) ==
      0) {
    RTC_LOG(LS_ERROR) << "Desired YUV420 RT format not supported";
    return VA_STATUS_ERROR_INVALID_CONFIG;
  }

  context->config_attrib_num = 0;
  context->config_attrib[context->config_attrib_num].type =
      VAConfigAttribRTFormat;
  context->config_attrib[context->config_attrib_num].value =
      VA_RT_FORMAT_YUV420;
  context->config_attrib_num++;

  if (context->attrib[VAConfigAttribRateControl].value !=
      VA_ATTRIB_NOT_SUPPORTED) {
    int supported_rc = context->attrib[VAConfigAttribRateControl].value;
    int selected_rc = VA_RC_NONE;
    if (supported_rc & context->config.rc_mode) {
      selected_rc = context->config.rc_mode;
    } else if (supported_rc & VA_RC_CBR) {
      selected_rc = VA_RC_CBR;
    } else if (supported_rc & VA_RC_VBR) {
      selected_rc = VA_RC_VBR;
    } else if (supported_rc & VA_RC_CQP) {
      selected_rc = VA_RC_CQP;
    }
    RTC_LOG(LS_INFO) << "HEVC VAAPI rate-control support mask=0x" << std::hex
                     << supported_rc << " selected=0x" << selected_rc
                     << std::dec;
    context->config_attrib[context->config_attrib_num].type =
        VAConfigAttribRateControl;
    context->config_attrib[context->config_attrib_num].value = selected_rc;
    context->config.rc_mode = selected_rc;
    context->config_attrib_num++;
  }

#if VA_CHECK_VERSION(1, 13, 0)
  if (context->attrib[VAConfigAttribEncHEVCFeatures].value !=
      VA_ATTRIB_NOT_SUPPORTED) {
    context->hevc_features =
        context->attrib[VAConfigAttribEncHEVCFeatures].value;
    RTC_LOG(LS_INFO) << "HEVC VAAPI feature mask=0x" << std::hex
                     << context->hevc_features << std::dec;
  } else {
    RTC_LOG(LS_WARNING)
        << "HEVC VAAPI feature caps not advertised; using conservative "
           "defaults";
  }

  if (context->attrib[VAConfigAttribEncHEVCBlockSizes].value !=
      VA_ATTRIB_NOT_SUPPORTED) {
    context->hevc_block_sizes =
        context->attrib[VAConfigAttribEncHEVCBlockSizes].value;
    VAConfigAttribValEncHEVCBlockSizes block_sizes = {};
    block_sizes.value = context->hevc_block_sizes;
    context->ctu_size =
        1u << (block_sizes.bits.log2_max_coding_tree_block_size_minus3 + 3);
    context->min_cb_size =
        1u << (block_sizes.bits.log2_min_luma_coding_block_size_minus3 + 3);
    RTC_LOG(LS_INFO) << "HEVC VAAPI block-size mask=0x" << std::hex
                     << context->hevc_block_sizes << std::dec
                     << " ctu_size=" << context->ctu_size
                     << " min_cb_size=" << context->min_cb_size;
  } else {
    RTC_LOG(LS_WARNING)
        << "HEVC VAAPI block-size caps not advertised; using guessed "
           "defaults";
  }
#endif

  if (context->attrib[VAConfigAttribEncPackedHeaders].value !=
      VA_ATTRIB_NOT_SUPPORTED) {
    const uint32_t desired_packed_headers =
        VA_ENC_PACKED_HEADER_SEQUENCE | VA_ENC_PACKED_HEADER_SLICE |
        VA_ENC_PACKED_HEADER_MISC;
    context->packed_headers =
        context->attrib[VAConfigAttribEncPackedHeaders].value &
        desired_packed_headers;
    RTC_LOG(LS_INFO) << "HEVC VAAPI packed-header support mask=0x" << std::hex
                     << context->attrib[VAConfigAttribEncPackedHeaders].value
                     << " usable=0x" << context->packed_headers << std::dec;
    if ((context->packed_headers & VA_ENC_PACKED_HEADER_SEQUENCE) == 0 ||
        (context->packed_headers & VA_ENC_PACKED_HEADER_SLICE) == 0) {
      RTC_LOG(LS_WARNING)
          << "HEVC VAAPI driver cannot accept all FFmpeg-style packed "
             "sequence/slice headers; driver-generated headers will be used";
    }
  } else {
    RTC_LOG(LS_WARNING)
        << "HEVC VAAPI packed headers are not advertised; "
           "driver-generated headers will be used";
  }

  return VA_STATUS_SUCCESS;
}

static int setup_encode(VA265Context* context) {
  VAStatus va_status = vaCreateConfig(
      context->va_dpy, context->config.h265_profile,
      (VAEntrypoint)context->selected_entrypoint, &context->config_attrib[0],
      context->config_attrib_num, &context->config_id);
  if (context->config_id == VA_INVALID_ID || va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaCreateConfig failed va_status = " << va_status;
    return -1;
  }

  va_status =
      vaCreateSurfaces(context->va_dpy, VA_RT_FORMAT_YUV420,
                       context->frame_width_aligned,
                       context->frame_height_aligned, &context->src_surface[0],
                       H265_SURFACE_NUM, NULL, 0);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaCreateSurfaces failed va_status = " << va_status;
    return -1;
  }

  va_status =
      vaCreateSurfaces(context->va_dpy, VA_RT_FORMAT_YUV420,
                       context->frame_width_aligned,
                       context->frame_height_aligned, &context->ref_surface[0],
                       H265_SURFACE_NUM, NULL, 0);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaCreateSurfaces failed va_status = " << va_status;
    return -1;
  }

  VASurfaceID tmp_surfaceid[2 * H265_SURFACE_NUM];
  memcpy(tmp_surfaceid, context->src_surface,
         H265_SURFACE_NUM * sizeof(VASurfaceID));
  memcpy(tmp_surfaceid + H265_SURFACE_NUM, context->ref_surface,
         H265_SURFACE_NUM * sizeof(VASurfaceID));

  va_status = vaCreateContext(
      context->va_dpy, context->config_id, context->frame_width_aligned,
      context->frame_height_aligned, VA_PROGRESSIVE, tmp_surfaceid,
      2 * H265_SURFACE_NUM, &context->context_id);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaCreateContext failed va_status = " << va_status;
    return -1;
  }

  int codedbuf_size =
      context->frame_width_aligned * context->frame_height_aligned * 3 +
      (1 << 16);
  for (int i = 0; i < H265_SURFACE_NUM; i++) {
    va_status = vaCreateBuffer(context->va_dpy, context->context_id,
                               VAEncCodedBufferType, codedbuf_size, 1, NULL,
                               &context->coded_buf[i]);
    if (va_status != VA_STATUS_SUCCESS) {
      RTC_LOG(LS_ERROR) << "vaCreateBuffer failed va_status = " << va_status;
      return -1;
    }
  }

  return 0;
}

static int add_sequence_buffers(VA265Context* context,
                                std::vector<VABufferID>* buffers) {
  memset(&context->seq_param, 0, sizeof(context->seq_param));
  context->seq_param.general_profile_idc = 1;
  context->seq_param.general_level_idc = 120;
  context->seq_param.general_tier_flag = 0;
  context->seq_param.intra_period = context->config.intra_period;
  context->seq_param.intra_idr_period = context->config.intra_idr_period;
  context->seq_param.ip_period = context->config.ip_period;
  context->seq_param.bits_per_second = context->config.bitrate;
  context->seq_param.pic_width_in_luma_samples =
      context->frame_width_aligned;
  context->seq_param.pic_height_in_luma_samples =
      context->frame_height_aligned;

#if VA_CHECK_VERSION(1, 13, 0)
  VAConfigAttribValEncHEVCFeatures features = {};
  features.value = context->hevc_features;
  VAConfigAttribValEncHEVCBlockSizes block_sizes = {};
  block_sizes.value = context->hevc_block_sizes;
#endif

  context->seq_param.seq_fields.bits.chroma_format_idc = 1;
  context->seq_param.seq_fields.bits.bit_depth_luma_minus8 = 0;
  context->seq_param.seq_fields.bits.bit_depth_chroma_minus8 = 0;
  context->seq_param.seq_fields.bits.low_delay_seq = 1;
  context->seq_param.seq_fields.bits.strong_intra_smoothing_enabled_flag =
#if VA_CHECK_VERSION(1, 13, 0)
      context->hevc_features
          ? va_feature_enabled(features.bits.strong_intra_smoothing)
          :
#endif
          1;
  context->seq_param.seq_fields.bits.amp_enabled_flag =
#if VA_CHECK_VERSION(1, 13, 0)
      context->hevc_features ? va_feature_enabled(features.bits.amp) :
#endif
                             1;
  context->seq_param.seq_fields.bits.sample_adaptive_offset_enabled_flag =
#if VA_CHECK_VERSION(1, 13, 0)
      context->hevc_features ? va_feature_enabled(features.bits.sao) :
#endif
                             0;
  context->seq_param.seq_fields.bits.sps_temporal_mvp_enabled_flag =
#if VA_CHECK_VERSION(1, 13, 0)
      context->hevc_features ? va_feature_enabled(features.bits.temporal_mvp) :
#endif
                             0;
  context->seq_param.seq_fields.bits.pcm_enabled_flag =
#if VA_CHECK_VERSION(1, 13, 0)
      context->hevc_features ? va_feature_enabled(features.bits.pcm) :
#endif
                             0;
  context->seq_param.log2_min_luma_coding_block_size_minus3 = 0;
  context->seq_param.log2_diff_max_min_luma_coding_block_size = 2;
  context->seq_param.log2_min_transform_block_size_minus2 = 0;
  context->seq_param.log2_diff_max_min_transform_block_size = 3;
#if VA_CHECK_VERSION(1, 13, 0)
  if (context->hevc_block_sizes) {
    context->seq_param.log2_min_luma_coding_block_size_minus3 =
        block_sizes.bits.log2_min_luma_coding_block_size_minus3;
    context->seq_param.log2_diff_max_min_luma_coding_block_size =
        block_sizes.bits.log2_max_coding_tree_block_size_minus3 -
        block_sizes.bits.log2_min_luma_coding_block_size_minus3;
    context->seq_param.log2_min_transform_block_size_minus2 =
        block_sizes.bits.log2_min_luma_transform_block_size_minus2;
    context->seq_param.log2_diff_max_min_transform_block_size =
        block_sizes.bits.log2_max_luma_transform_block_size_minus2 -
        block_sizes.bits.log2_min_luma_transform_block_size_minus2;
    context->seq_param.max_transform_hierarchy_depth_inter =
        block_sizes.bits.max_max_transform_hierarchy_depth_inter;
    context->seq_param.max_transform_hierarchy_depth_intra =
        block_sizes.bits.max_max_transform_hierarchy_depth_intra;
  } else
#endif
  {
    context->seq_param.max_transform_hierarchy_depth_inter = 3;
    context->seq_param.max_transform_hierarchy_depth_intra = 3;
  }
  context->seq_param.vui_parameters_present_flag = 0;

  VABufferID seq_param_buf;
  VAStatus va_status = vaCreateBuffer(
      context->va_dpy, context->context_id, VAEncSequenceParameterBufferType,
      sizeof(context->seq_param), 1, &context->seq_param, &seq_param_buf);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaCreateBuffer failed va_status = " << va_status;
    return -1;
  }
  buffers->push_back(seq_param_buf);

  VABufferID rc_param_buf;
  va_status = vaCreateBuffer(
      context->va_dpy, context->context_id, VAEncMiscParameterBufferType,
      sizeof(VAEncMiscParameterBuffer) + sizeof(VAEncMiscParameterRateControl),
      1, NULL, &rc_param_buf);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaCreateBuffer failed va_status = " << va_status;
    return -1;
  }

  VAEncMiscParameterBuffer* misc_param = nullptr;
  vaMapBuffer(context->va_dpy, rc_param_buf, (void**)&misc_param);
  misc_param->type = VAEncMiscParameterTypeRateControl;
  auto* misc_rate_ctrl =
      reinterpret_cast<VAEncMiscParameterRateControl*>(misc_param->data);
  memset(misc_rate_ctrl, 0, sizeof(*misc_rate_ctrl));
  misc_rate_ctrl->bits_per_second = context->config.bitrate;
  misc_rate_ctrl->target_percentage = 66;
  misc_rate_ctrl->window_size = 1000;
  misc_rate_ctrl->initial_qp = context->config.initial_qp;
  misc_rate_ctrl->min_qp = context->config.minimal_qp;
  misc_rate_ctrl->rc_flags.bits.disable_frame_skip = true;
  vaUnmapBuffer(context->va_dpy, rc_param_buf);
  buffers->push_back(rc_param_buf);
  return 0;
}

static int add_picture_buffer(VA265Context* context,
                              std::vector<VABufferID>* buffers) {
  memset(&context->pic_param, 0, sizeof(context->pic_param));
  context->pic_param.decoded_curr_pic.picture_id =
      context->ref_surface[context->current_frame_display % H265_SURFACE_NUM];
  context->pic_param.decoded_curr_pic.pic_order_cnt =
      context->current_frame_display - context->current_idr_display;
  context->pic_param.decoded_curr_pic.flags = 0;
  context->current_curr_pic = context->pic_param.decoded_curr_pic;

  for (int i = 0; i < H265_MAX_DPB_SIZE; i++) {
    context->pic_param.reference_frames[i] =
        (i < static_cast<int>(context->num_short_term))
            ? context->reference_frames[i]
            : VAPictureHEVC{};
    if (i >= static_cast<int>(context->num_short_term)) {
      init_invalid_picture(&context->pic_param.reference_frames[i]);
    }
  }

  context->pic_param.coded_buf =
      context->coded_buf[context->current_frame_display % H265_SURFACE_NUM];
  context->pic_param.collocated_ref_pic_index = 0xff;
  context->pic_param.last_picture = 0;
  context->pic_param.pic_init_qp = context->config.initial_qp;
  context->pic_param.diff_cu_qp_delta_depth =
      context->seq_param.log2_diff_max_min_luma_coding_block_size;
  context->pic_param.log2_parallel_merge_level_minus2 = 0;
  context->pic_param.ctu_max_bitsize_allowed = 0;
  context->pic_param.num_ref_idx_l0_default_active_minus1 = 0;
  context->pic_param.num_ref_idx_l1_default_active_minus1 = 0;
  context->pic_param.slice_pic_parameter_set_id = 0;
  context->pic_param.nal_unit_type =
      (context->current_frame_type == kH265FrameIdr) ? 19 : 1;
  context->pic_param.pic_fields.bits.idr_pic_flag =
      (context->current_frame_type == kH265FrameIdr);
  context->pic_param.pic_fields.bits.coding_type =
      (context->current_frame_type == kH265FrameP) ? 2 : 1;
  context->pic_param.pic_fields.bits.reference_pic_flag = 1;
#if VA_CHECK_VERSION(1, 13, 0)
  if (context->hevc_features) {
    VAConfigAttribValEncHEVCFeatures features = {};
    features.value = context->hevc_features;
    context->pic_param.pic_fields.bits.cu_qp_delta_enabled_flag =
        context->config.rc_mode != VA_RC_CQP &&
        va_feature_enabled(features.bits.cu_qp_delta);
    context->pic_param.pic_fields.bits.transform_skip_enabled_flag =
        va_feature_enabled(features.bits.transform_skip);
  }
#endif
  context->pic_param.pic_fields.bits.pps_loop_filter_across_slices_enabled_flag =
      1;

  VABufferID pic_param_buf;
  VAStatus va_status = vaCreateBuffer(
      context->va_dpy, context->context_id, VAEncPictureParameterBufferType,
      sizeof(context->pic_param), 1, &context->pic_param, &pic_param_buf);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaCreateBuffer failed va_status = " << va_status;
    return -1;
  }

  buffers->push_back(pic_param_buf);
  return 0;
}

static int add_slice_buffer(VA265Context* context,
                            std::vector<VABufferID>* buffers) {
  memset(&context->slice_param, 0, sizeof(context->slice_param));
  context->slice_param.slice_segment_address = 0;
  context->slice_param.num_ctu_in_slice =
      ((context->frame_width_aligned + context->ctu_size - 1) /
       context->ctu_size) *
      ((context->frame_height_aligned + context->ctu_size - 1) /
       context->ctu_size);
  context->slice_param.slice_type =
      (context->current_frame_type == kH265FrameP) ? 1 : 2;
  context->slice_param.slice_pic_parameter_set_id = 0;
  context->slice_param.num_ref_idx_l0_active_minus1 = 0;
  context->slice_param.num_ref_idx_l1_active_minus1 = 0;
  context->slice_param.max_num_merge_cand = 5;
  context->slice_param.slice_qp_delta = 0;
  context->slice_param.slice_fields.bits.last_slice_of_pic_flag = 1;
  context->slice_param.slice_fields.bits.collocated_from_l0_flag = 1;
  context->slice_param.slice_fields.bits.slice_temporal_mvp_enabled_flag =
      context->seq_param.seq_fields.bits.sps_temporal_mvp_enabled_flag;
  context->slice_param.slice_fields.bits.slice_sao_luma_flag =
      context->seq_param.seq_fields.bits.sample_adaptive_offset_enabled_flag;
  context->slice_param.slice_fields.bits.slice_sao_chroma_flag =
      context->seq_param.seq_fields.bits.sample_adaptive_offset_enabled_flag;
  context->slice_param.slice_fields.bits.slice_loop_filter_across_slices_enabled_flag =
      1;

  for (int i = 0; i < H265_MAX_DPB_SIZE; i++) {
    init_invalid_picture(&context->slice_param.ref_pic_list0[i]);
    init_invalid_picture(&context->slice_param.ref_pic_list1[i]);
  }

  if (context->current_frame_type == kH265FrameP &&
      context->num_short_term > 0) {
    context->slice_param.ref_pic_list0[0] = context->reference_frames[0];
  }

  VABufferID slice_param_buf;
  VAStatus va_status = vaCreateBuffer(
      context->va_dpy, context->context_id, VAEncSliceParameterBufferType,
      sizeof(context->slice_param), 1, &context->slice_param, &slice_param_buf);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaCreateBuffer failed va_status = " << va_status;
    return -1;
  }

  buffers->push_back(slice_param_buf);
  return 0;
}

static void update_reference_frames(VA265Context* context) {
  context->current_curr_pic.flags = VA_PICTURE_HEVC_RPS_ST_CURR_BEFORE;
  if (context->num_short_term < H265_MAX_DPB_SIZE) {
    context->num_short_term++;
  }
  for (int i = context->num_short_term - 1; i > 0; i--) {
    context->reference_frames[i] = context->reference_frames[i - 1];
  }
  context->reference_frames[0] = context->current_curr_pic;
}

namespace livekit_ffi {

VaapiH265EncoderWrapper::VaapiH265EncoderWrapper()
    : va_display_(std::make_unique<VaapiDisplay>()) {
  context_ = std::make_unique<VA265Context>();
  memset(context_.get(), 0, sizeof(VA265Context));
  for (int i = 0; i < H265_MAX_DPB_SIZE; i++) {
    init_invalid_picture(&context_->reference_frames[i]);
  }
}

VaapiH265EncoderWrapper::~VaapiH265EncoderWrapper() {}

void VaapiH265EncoderWrapper::Destroy() {
  if (context_->va_dpy) {
    vaDestroySurfaces(context_->va_dpy, &context_->src_surface[0],
                      H265_SURFACE_NUM);
    vaDestroySurfaces(context_->va_dpy, &context_->ref_surface[0],
                      H265_SURFACE_NUM);
    for (int i = 0; i < H265_SURFACE_NUM; i++) {
      if (context_->coded_buf[i] != VA_INVALID_ID) {
        vaDestroyBuffer(context_->va_dpy, context_->coded_buf[i]);
      }
    }
    if (context_->context_id != VA_INVALID_ID) {
      vaDestroyContext(context_->va_dpy, context_->context_id);
    }
    if (context_->config_id != VA_INVALID_ID) {
      vaDestroyConfig(context_->va_dpy, context_->config_id);
    }
  }

  if (va_display_->isOpen()) {
    vaTerminate(va_display_->display());
    va_display_->Close();
  }

  memset(context_.get(), 0, sizeof(VA265Context));
  for (int i = 0; i < H265_MAX_DPB_SIZE; i++) {
    init_invalid_picture(&context_->reference_frames[i]);
  }
  initialized_ = false;
}

bool VaapiH265EncoderWrapper::Initialize(int width,
                                         int height,
                                         int bitrate,
                                         int intra_period,
                                         int idr_period,
                                         int ip_period,
                                         int frame_rate,
                                         VAProfile profile,
                                         int rc_mode) {
  context_->config.frame_width = width;
  context_->config.frame_height = height;
  context_->config.frame_rate = frame_rate;
  context_->config.bitrate = bitrate;
  context_->config.initial_qp = 26;
  context_->config.minimal_qp = 0;
  context_->config.intra_period = intra_period;
  context_->config.intra_idr_period = idr_period;
  context_->config.ip_period = std::max(ip_period, 1);
  context_->config.rc_mode = rc_mode;
  context_->config.h265_profile = profile;
  context_->requested_entrypoint = context_->selected_entrypoint = -1;
  context_->context_id = VA_INVALID_ID;
  context_->config_id = VA_INVALID_ID;
  context_->ctu_size = kCtuSize;
  context_->min_cb_size = 16;
  context_->hevc_features = 0;
  context_->hevc_block_sizes = 0;
  context_->packed_headers = 0;
  for (int i = 0; i < H265_SURFACE_NUM; i++) {
    context_->coded_buf[i] = VA_INVALID_ID;
  }

  if (context_->config.bitrate == 0) {
    context_->config.bitrate = context_->config.frame_width *
                               context_->config.frame_height * 12 *
                               context_->config.frame_rate / 50;
  }

  if (!va_display_->isOpen() && !va_display_->Open()) {
    return false;
  }

  if (init_va(context_.get(), va_display_->display()) != VA_STATUS_SUCCESS) {
    if (va_display_->isOpen()) {
      vaTerminate(va_display_->display());
      va_display_->Close();
    }
    return false;
  }

  const uint32_t alignment =
      std::max<uint32_t>(context_->ctu_size, context_->min_cb_size);
  context_->frame_width_aligned =
      align_up(context_->config.frame_width, alignment);
  context_->frame_height_aligned =
      align_up(context_->config.frame_height, alignment);
  RTC_LOG(LS_INFO) << "HEVC VAAPI frame size: visible="
                   << context_->config.frame_width << "x"
                   << context_->config.frame_height << " coded="
                   << context_->frame_width_aligned << "x"
                   << context_->frame_height_aligned
                   << " alignment=" << alignment
                   << " ctu_size=" << context_->ctu_size
                   << " min_cb_size=" << context_->min_cb_size;

  if (setup_encode(context_.get()) != 0) {
    if (context_->va_dpy) {
      vaTerminate(context_->va_dpy);
      context_->va_dpy = nullptr;
    }
    if (va_display_->isOpen()) {
      va_display_->Close();
    }
    return false;
  }

  initialized_ = true;
  return true;
}

bool VaapiH265EncoderWrapper::Encode(int fourcc,
                                     const uint8_t* y,
                                     int stride_y,
                                     const uint8_t* u,
                                     int stride_u,
                                     const uint8_t* v,
                                     int stride_v,
                                     bool forceIDR,
                                     std::vector<uint8_t>& encoded) {
  if (forceIDR) {
    context_->current_frame_display = 0;
    context_->current_frame_encoding = 0;
    context_->current_idr_display = 0;
  }

  VASurfaceID surface =
      context_->src_surface[context_->current_frame_encoding % H265_SURFACE_NUM];
  int retv = upload_surface_yuv(
      context_->va_dpy, surface, fourcc, context_->config.frame_width,
      context_->config.frame_height, y, stride_y, u, stride_u, v, stride_v);
  if (retv != 0) {
    RTC_LOG(LS_ERROR) << "Failed to upload surface";
    return false;
  }

  h265_encoding2display_order(
      context_->current_frame_encoding, context_->config.intra_period,
      context_->config.intra_idr_period, &context_->current_frame_display,
      &context_->current_frame_type);

  if (context_->current_frame_type == kH265FrameIdr) {
    context_->num_short_term = 0;
    context_->current_idr_display = context_->current_frame_display;
    for (int i = 0; i < H265_MAX_DPB_SIZE; i++) {
      init_invalid_picture(&context_->reference_frames[i]);
    }
  }

  std::vector<VABufferID> render_buffers;
  render_buffers.reserve(context_->current_frame_type == kH265FrameIdr ? 4 : 2);
  if (context_->current_frame_type == kH265FrameIdr) {
    if (add_sequence_buffers(context_.get(), &render_buffers) != 0) {
      destroy_buffers(context_->va_dpy, render_buffers);
      return false;
    }
  }
  if (add_picture_buffer(context_.get(), &render_buffers) != 0 ||
      add_slice_buffer(context_.get(), &render_buffers) != 0) {
    destroy_buffers(context_->va_dpy, render_buffers);
    return false;
  }

  VAStatus va_status = vaBeginPicture(
      context_->va_dpy, context_->context_id,
      context_->src_surface[context_->current_frame_display % H265_SURFACE_NUM]);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaBeginPicture failed va_status = " << va_status;
    destroy_buffers(context_->va_dpy, render_buffers);
    return false;
  }

  va_status =
      vaRenderPicture(context_->va_dpy, context_->context_id,
                      render_buffers.data(),
                      static_cast<int>(render_buffers.size()));
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaRenderPicture failed va_status = " << va_status;
    vaEndPicture(context_->va_dpy, context_->context_id);
    destroy_buffers(context_->va_dpy, render_buffers);
    return false;
  }

  va_status = vaEndPicture(context_->va_dpy, context_->context_id);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaEndPicture failed va_status = " << va_status;
    destroy_buffers(context_->va_dpy, render_buffers);
    return false;
  }
  destroy_buffers(context_->va_dpy, render_buffers);

  va_status = vaSyncSurface(
      context_->va_dpy,
      context_->src_surface[context_->current_frame_display % H265_SURFACE_NUM]);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaSyncSurface failed va_status = " << va_status;
    return false;
  }

  VACodedBufferSegment* buf_list = NULL;
  encoded.clear();
  va_status = vaMapBuffer(
      context_->va_dpy,
      context_->coded_buf[context_->current_frame_display % H265_SURFACE_NUM],
      reinterpret_cast<void**>(&buf_list));
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaMapBuffer failed va_status = " << va_status;
    return false;
  }
  while (buf_list != NULL) {
    append_annex_b_segment(reinterpret_cast<const uint8_t*>(buf_list->buf),
                           buf_list->size, &encoded);
    buf_list = reinterpret_cast<VACodedBufferSegment*>(buf_list->next);
  }
  vaUnmapBuffer(
      context_->va_dpy,
      context_->coded_buf[context_->current_frame_display % H265_SURFACE_NUM]);

  if (!encoded.empty() && !contains_start_code(encoded)) {
    RTC_LOG(LS_ERROR)
        << "VAAPI H265 encoder produced a bitstream without Annex-B NAL "
           "start codes";
    return false;
  }

  update_reference_frames(context_.get());
  context_->current_frame_encoding++;

  return true;
}

}  // namespace livekit_ffi
