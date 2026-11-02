#include "vaapi_display_drm.h"

#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

#include <vector>

#ifdef IN_LIBVA
#include "va/drm/va_drm.h"
#else
#include <va/va_drm.h>
#endif

#include "rtc_base/logging.h"

static bool check_h264_encoding_support(VADisplay va_display) {
  VAProfile profile_list[] = {VAProfileH264High, VAProfileH264Main,
                              VAProfileH264ConstrainedBaseline};

  VAProfile h264_profile = VAProfileH264ConstrainedBaseline;
  int slice_entrypoint;
  bool support_encode = false;
  int selected_entrypoint = -1;
  uint32_t i;

  if (!va_display) {
    return false;
  }

  const int max_entrypoints = vaMaxNumEntrypoints(va_display);
  if (max_entrypoints <= 0) {
    RTC_LOG(LS_ERROR) << "VAAPI reported no entrypoints";
    return false;
  }
  std::vector<VAEntrypoint> entrypoints(max_entrypoints);

  /* use the highest profile */
  for (i = 0; i < sizeof(profile_list) / sizeof(profile_list[0]); i++) {
    if ((h264_profile != ~0) && h264_profile != profile_list[i])
      continue;

    h264_profile = profile_list[i];
    int num_entrypoints = max_entrypoints;
    vaQueryConfigEntrypoints(va_display, h264_profile, entrypoints.data(),
                             &num_entrypoints);
    for (slice_entrypoint = 0; slice_entrypoint < num_entrypoints;
         slice_entrypoint++) {
      if ((entrypoints[slice_entrypoint] == VAEntrypointEncSlice) ||
          (entrypoints[slice_entrypoint] == VAEntrypointEncSliceLP)) {
        support_encode = true;
        selected_entrypoint = entrypoints[slice_entrypoint];
        break;
      }
    }
    if (support_encode) {
      RTC_LOG(LS_INFO) << "Using EntryPoint - " << selected_entrypoint;
      break;
    }
  }

  if (support_encode) {
    RTC_LOG(LS_INFO) << "Supported H264 Encoder, Using EntryPoint - "
                     << selected_entrypoint;
  } else {
    RTC_LOG(LS_ERROR)
        << "Can't find VAEntrypointEncSlice or VAEntrypointEncSliceLP for "
           "H264 profiles";
    return false;
  }

  return true;
}

static VADisplay va_open_display_drm(int* drm_fd) {
  VADisplay va_dpy;
  int i;

  static const char* drm_device_paths[] = {"/dev/dri/renderD128",
                                           "/dev/dri/renderD129", NULL};
  for (i = 0; drm_device_paths[i]; i++) {
    *drm_fd = open(drm_device_paths[i], O_RDWR);
    if (*drm_fd < 0)
      continue;

    va_dpy = vaGetDisplayDRM(*drm_fd);
    if (!va_dpy) {
      close(*drm_fd);
      *drm_fd = -1;
      continue;
    }

    vaSetErrorCallback(va_dpy, NULL, NULL);
    vaSetInfoCallback(va_dpy, NULL, NULL);

    int major_ver = 0;
    int minor_ver = 0;
    VAStatus va_status = vaInitialize(va_dpy, &major_ver, &minor_ver);
    if (va_status == VA_STATUS_SUCCESS && check_h264_encoding_support(va_dpy)) {
      RTC_LOG(LS_INFO) << "Initialized VAAPI successfully with version " << major_ver << "." << minor_ver;
      return va_dpy;
    }
    RTC_LOG(LS_ERROR) << "Failed to initialize VAAPI with status: "
                      << vaErrorStr(va_status);
    vaTerminate(va_dpy);
    close(*drm_fd);
    *drm_fd = -1;
  }
  return NULL;
}

namespace livekit_ffi {

VaapiDisplayDrm::~VaapiDisplayDrm() {
  Close();
}

bool VaapiDisplayDrm::Open() {
  Close();
  va_display_ = va_open_display_drm(&drm_fd_);
  if (!va_display_) {
    RTC_LOG(LS_ERROR) << "Failed to open VA drm display. Maybe the AMD video "
                         "driver or libva-dev/libdrm-dev is not installed?";
    return false;
  }
  return true;
}

bool VaapiDisplayDrm::isOpen() const {
  return va_display_ != nullptr;
}

void VaapiDisplayDrm::Close() {
  if (va_display_) {
    vaTerminate(va_display_);
    va_display_ = nullptr;
  }
  if (drm_fd_ >= 0) {
    close(drm_fd_);
    drm_fd_ = -1;
  }
}

}  // namespace livekit_ffi
