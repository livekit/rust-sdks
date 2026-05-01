#include "vaapi_h265_encoder_wrapper.h"

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <algorithm>
#include <memory>

#include "rtc_base/logging.h"

#define VA_FOURCC_I420 0x30323449

static const uint32_t kCtuSize = 32;

static const int kH265FrameP = 0;
static const int kH265FrameI = 2;
static const int kH265FrameIdr = 7;

static int upload_surface_yuv(VADisplay va_dpy,
                              VASurfaceID surface_id,
                              int src_fourcc,
                              int src_width,
                              int src_height,
                              const uint8_t* src_Y,
                              const uint8_t* src_U,
                              const uint8_t* src_V) {
  VAImageFormat kImageFormatI420 = {
      .fourcc = VA_FOURCC_I420,
      .byte_order = VA_LSB_FIRST,
      .bits_per_pixel = 12,
  };

  VAImage surface_image;
  uint8_t *surface_p = NULL, *Y_start = NULL, *U_start = NULL;
  int Y_pitch = 0, U_pitch = 0, row;
  VAStatus va_status;

  va_status = vaDeriveImage(va_dpy, surface_id, &surface_image);
  if (va_status != VA_STATUS_SUCCESS) {
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
    case VA_FOURCC_NV12:
    case VA_FOURCC_I420:
      U_start = surface_p + surface_image.offsets[1];
      U_pitch = surface_image.pitches[1];
      break;
    case VA_FOURCC_YV12:
      U_start = surface_p + surface_image.offsets[2];
      U_pitch = surface_image.pitches[2];
      break;
    default:
      RTC_LOG(LS_ERROR) << "Unsupported VA surface image format";
      vaUnmapBuffer(va_dpy, surface_image.buf);
      vaDestroyImage(va_dpy, surface_image.image_id);
      return -1;
  }

  for (row = 0; row < src_height; row++) {
    memcpy(Y_start + row * Y_pitch, src_Y + row * src_width, src_width);
  }

  for (row = 0; row < src_height / 2; row++) {
    uint8_t* U_row = U_start + row * U_pitch;
    if (src_fourcc == VA_FOURCC_NV12) {
      memcpy(U_row, src_U + row * src_width, src_width);
      continue;
    }

    const uint8_t* u_ptr = src_U + row * (src_width / 2);
    const uint8_t* v_ptr = src_V + row * (src_width / 2);
    if (src_fourcc == VA_FOURCC_YV12) {
      std::swap(u_ptr, v_ptr);
    }
    for (int col = 0; col < src_width / 2; col++) {
      U_row[2 * col] = u_ptr[col];
      U_row[2 * col + 1] = v_ptr[col];
    }
  }

  vaUnmapBuffer(va_dpy, surface_image.buf);
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
    int selected_rc =
        (supported_rc & context->config.rc_mode) ? context->config.rc_mode
                                                 : VA_RC_NONE;
    context->config_attrib[context->config_attrib_num].type =
        VAConfigAttribRateControl;
    context->config_attrib[context->config_attrib_num].value = selected_rc;
    context->config.rc_mode = selected_rc;
    context->config_attrib_num++;
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
      context->frame_width_aligned * context->frame_height_aligned * 3;
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

static int render_sequence(VA265Context* context) {
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
  context->seq_param.seq_fields.bits.chroma_format_idc = 1;
  context->seq_param.seq_fields.bits.low_delay_seq = 1;
  context->seq_param.seq_fields.bits.strong_intra_smoothing_enabled_flag = 1;
  context->seq_param.seq_fields.bits.sps_temporal_mvp_enabled_flag = 0;
  context->seq_param.log2_min_luma_coding_block_size_minus3 = 0;
  context->seq_param.log2_diff_max_min_luma_coding_block_size = 2;
  context->seq_param.log2_min_transform_block_size_minus2 = 0;
  context->seq_param.log2_diff_max_min_transform_block_size = 3;
  context->seq_param.max_transform_hierarchy_depth_inter = 2;
  context->seq_param.max_transform_hierarchy_depth_intra = 2;
  context->seq_param.vui_parameters_present_flag = 1;
  context->seq_param.vui_fields.bits.vui_timing_info_present_flag = 1;
  context->seq_param.vui_num_units_in_tick = 1;
  context->seq_param.vui_time_scale = context->config.frame_rate;

  VABufferID seq_param_buf;
  VAStatus va_status = vaCreateBuffer(
      context->va_dpy, context->context_id, VAEncSequenceParameterBufferType,
      sizeof(context->seq_param), 1, &context->seq_param, &seq_param_buf);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaCreateBuffer failed va_status = " << va_status;
    return -1;
  }

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

  VABufferID render_id[2] = {seq_param_buf, rc_param_buf};
  va_status =
      vaRenderPicture(context->va_dpy, context->context_id, render_id, 2);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaRenderPicture failed va_status = " << va_status;
    return -1;
  }
  return 0;
}

static int render_picture(VA265Context* context) {
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
  context->pic_param.pic_fields.bits.cu_qp_delta_enabled_flag = 1;
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

  va_status =
      vaRenderPicture(context->va_dpy, context->context_id, &pic_param_buf, 1);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaRenderPicture failed va_status = " << va_status;
    return -1;
  }
  return 0;
}

static int render_slice(VA265Context* context) {
  memset(&context->slice_param, 0, sizeof(context->slice_param));
  context->slice_param.slice_segment_address = 0;
  context->slice_param.num_ctu_in_slice =
      (context->frame_width_aligned / kCtuSize) *
      (context->frame_height_aligned / kCtuSize);
  context->slice_param.slice_type =
      (context->current_frame_type == kH265FrameP) ? 1 : 2;
  context->slice_param.slice_pic_parameter_set_id = 0;
  context->slice_param.num_ref_idx_l0_active_minus1 = 0;
  context->slice_param.num_ref_idx_l1_active_minus1 = 0;
  context->slice_param.max_num_merge_cand = 5;
  context->slice_param.slice_fields.bits.last_slice_of_pic_flag = 1;
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

  va_status = vaRenderPicture(context->va_dpy, context->context_id,
                              &slice_param_buf, 1);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaRenderPicture failed va_status = " << va_status;
    return -1;
  }
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

  if (context_->encoded_buffer) {
    free(context_->encoded_buffer);
    context_->encoded_buffer = nullptr;
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
  for (int i = 0; i < H265_SURFACE_NUM; i++) {
    context_->coded_buf[i] = VA_INVALID_ID;
  }

  if (context_->config.bitrate == 0) {
    context_->config.bitrate = context_->config.frame_width *
                               context_->config.frame_height * 12 *
                               context_->config.frame_rate / 50;
  }

  context_->frame_width_aligned =
      (context_->config.frame_width + kCtuSize - 1) & ~(kCtuSize - 1);
  context_->frame_height_aligned =
      (context_->config.frame_height + kCtuSize - 1) & ~(kCtuSize - 1);

  context_->encoded_buffer = reinterpret_cast<uint8_t*>(
      malloc(context_->frame_width_aligned * context_->frame_height_aligned *
             3));
  if (!context_->encoded_buffer) {
    return false;
  }

  if (!va_display_->isOpen() && !va_display_->Open()) {
    free(context_->encoded_buffer);
    context_->encoded_buffer = nullptr;
    return false;
  }

  if (init_va(context_.get(), va_display_->display()) != VA_STATUS_SUCCESS) {
    free(context_->encoded_buffer);
    context_->encoded_buffer = nullptr;
    if (va_display_->isOpen()) {
      vaTerminate(va_display_->display());
      va_display_->Close();
    }
    return false;
  }

  if (setup_encode(context_.get()) != 0) {
    free(context_->encoded_buffer);
    context_->encoded_buffer = nullptr;
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
                                     const uint8_t* u,
                                     const uint8_t* v,
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
      context_->config.frame_height, y, u, v);
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

  VAStatus va_status = vaBeginPicture(
      context_->va_dpy, context_->context_id,
      context_->src_surface[context_->current_frame_display % H265_SURFACE_NUM]);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaBeginPicture failed va_status = " << va_status;
    return false;
  }

  if (context_->current_frame_type == kH265FrameIdr) {
    if (render_sequence(context_.get()) != 0) {
      return false;
    }
  }
  if (render_picture(context_.get()) != 0 || render_slice(context_.get()) != 0) {
    return false;
  }

  va_status = vaEndPicture(context_->va_dpy, context_->context_id);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaEndPicture failed va_status = " << va_status;
    return false;
  }

  va_status = vaSyncSurface(
      context_->va_dpy,
      context_->src_surface[context_->current_frame_display % H265_SURFACE_NUM]);
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaSyncSurface failed va_status = " << va_status;
    return false;
  }

  VACodedBufferSegment* buf_list = NULL;
  uint32_t coded_size = 0;
  uint8_t* output = context_->encoded_buffer;
  va_status = vaMapBuffer(
      context_->va_dpy,
      context_->coded_buf[context_->current_frame_display % H265_SURFACE_NUM],
      reinterpret_cast<void**>(&buf_list));
  if (va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaMapBuffer failed va_status = " << va_status;
    return false;
  }
  while (buf_list != NULL) {
    memcpy(&output[coded_size], buf_list->buf, buf_list->size);
    coded_size += buf_list->size;
    buf_list = reinterpret_cast<VACodedBufferSegment*>(buf_list->next);
  }
  vaUnmapBuffer(
      context_->va_dpy,
      context_->coded_buf[context_->current_frame_display % H265_SURFACE_NUM]);

  update_reference_frames(context_.get());
  context_->current_frame_encoding++;

  encoded = std::vector<uint8_t>(output, output + coded_size);
  return true;
}

}  // namespace livekit_ffi
