#ifndef VAAPI_H265_ENCODER_WRAPPER_H_
#define VAAPI_H265_ENCODER_WRAPPER_H_

#include <stdbool.h>
#include <stdint.h>
#include <va/va.h>
#include <va/va_enc_hevc.h>

#include <memory>
#include <vector>

#if defined(WIN32)
#include "vaapi_display_win32.h"
using VaapiDisplay = livekit_ffi::VaapiDisplayWin32;
#elif defined(__linux__)
#include "vaapi_display_drm.h"
using VaapiDisplay = livekit_ffi::VaapiDisplayDrm;
#endif

#define H265_SURFACE_NUM 16
#define H265_MAX_DPB_SIZE 15

typedef struct {
  VAProfile h265_profile;
  int frame_width;
  int frame_height;
  int frame_rate;
  uint32_t bitrate;
  int initial_qp;
  int minimal_qp;
  int intra_period;
  int intra_idr_period;
  int ip_period;
  int rc_mode;
} VA265Config;

typedef struct {
  VADisplay va_dpy;

  VAConfigAttrib attrib[VAConfigAttribTypeMax];
  VAConfigAttrib config_attrib[VAConfigAttribTypeMax];
  int config_attrib_num;
  VASurfaceID src_surface[H265_SURFACE_NUM];
  VASurfaceID ref_surface[H265_SURFACE_NUM];
  VABufferID coded_buf[H265_SURFACE_NUM];
  VAConfigID config_id;
  VAContextID context_id;
  VAEncSequenceParameterBufferHEVC seq_param;
  VAEncPictureParameterBufferHEVC pic_param;
  VAEncSliceParameterBufferHEVC slice_param;
  VAPictureHEVC current_curr_pic;
  VAPictureHEVC reference_frames[H265_MAX_DPB_SIZE];

  int requested_entrypoint;
  int selected_entrypoint;

  uint32_t num_short_term;
  int frame_width_aligned;
  int frame_height_aligned;
  uint64_t current_frame_encoding;
  uint64_t current_frame_display;
  uint64_t current_idr_display;
  int current_frame_type;

  uint8_t* encoded_buffer;
  VA265Config config;
} VA265Context;

namespace livekit_ffi {

class VaapiH265EncoderWrapper {
 public:
  VaapiH265EncoderWrapper();
  ~VaapiH265EncoderWrapper();

  bool Initialize(int width,
                  int height,
                  int bitrate,
                  int intra_period,
                  int idr_period,
                  int ip_period,
                  int frame_rate,
                  VAProfile profile,
                  int rc_mode);

  bool Encode(int fourcc,
              const uint8_t* y,
              const uint8_t* u,
              const uint8_t* v,
              bool forceIDR,
              std::vector<uint8_t>& output);

  void UpdateRates(int frame_rate, int bitrate) {
    if (context_) {
      context_->config.frame_rate = frame_rate;
      context_->config.bitrate = bitrate;
    }
  }

  bool IsInitialized() const { return initialized_; }

  void Destroy();

 private:
  std::unique_ptr<VA265Context> context_;
  std::unique_ptr<VaapiDisplay> va_display_;
  bool initialized_ = false;
};

}  // namespace livekit_ffi

#endif  // VAAPI_H265_ENCODER_WRAPPER_H_
