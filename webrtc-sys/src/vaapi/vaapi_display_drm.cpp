#include "vaapi_display_drm.h"

#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

#ifdef IN_LIBVA
#include "va/drm/va_drm.h"
#else
#include <va/va_drm.h>
#endif

#include "rtc_base/logging.h"

static bool check_encoding_support(VADisplay va_display,
                                   const VAProfile* profile_list,
                                   int profile_count) {
  VAEntrypoint* entrypoints;
  int num_entrypoints, slice_entrypoint;
  bool support_encode = false;
  int selected_entrypoint = -1;
  int major_ver, minor_ver;
  VAStatus va_status;
  uint32_t i;

  if (!va_display) {
    return false;
  }

  va_status = vaInitialize(va_display, &major_ver, &minor_ver);

  if (major_ver < 0 || minor_ver < 0 || va_status != VA_STATUS_SUCCESS) {
    RTC_LOG(LS_ERROR) << "vaInitialize failed";
    return false;
  }

  num_entrypoints = vaMaxNumEntrypoints(va_display);
  entrypoints = new VAEntrypoint[num_entrypoints * sizeof(*entrypoints)];
  if (!entrypoints) {
    RTC_LOG(LS_ERROR) << "failed to allocate VA entrypoints";
    vaTerminate(va_display);
    return false;
  }

  for (i = 0; i < static_cast<uint32_t>(profile_count); i++) {
    vaQueryConfigEntrypoints(va_display, profile_list[i], entrypoints,
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
    RTC_LOG(LS_INFO) << "Supported VAAPI Encoder, Using EntryPoint - "
                     << selected_entrypoint;
  } else {
    RTC_LOG(LS_ERROR)
        << "Can't find VAEntrypointEncSlice or VAEntrypointEncSliceLP for "
           "requested VAAPI encode profiles";
    delete[] entrypoints;
    vaTerminate(va_display);
    return false;
  }

  delete[] entrypoints;
  vaTerminate(va_display);
  return true;
}

static bool check_h264_encoding_support(VADisplay va_display) {
  VAProfile profile_list[] = {VAProfileH264High, VAProfileH264Main,
                              VAProfileH264ConstrainedBaseline};
  return check_encoding_support(
      va_display, profile_list,
      static_cast<int>(sizeof(profile_list) / sizeof(profile_list[0])));
}

static bool check_h265_encoding_support(VADisplay va_display) {
  VAProfile profile_list[] = {VAProfileHEVCMain};
  return check_encoding_support(
      va_display, profile_list,
      static_cast<int>(sizeof(profile_list) / sizeof(profile_list[0])));
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
    vaSetErrorCallback(va_dpy, NULL, NULL);
    vaSetInfoCallback(va_dpy, NULL, NULL);
    if (va_dpy)
      return va_dpy;

    close(*drm_fd);
    *drm_fd = -1;
  }
  return NULL;
}

namespace livekit_ffi {

bool VaapiDisplayDrm::Open() {
  va_display_ = va_open_display_drm(&drm_fd_);
  if (!va_display_) {
    RTC_LOG(LS_ERROR) << "Failed to open VA drm display. Maybe the video "
                         "driver or libva-dev/libdrm-dev is not installed?";
    return false;
  }
  return true;
}

bool VaapiDisplayDrm::SupportsH264Encode() const {
  return check_h264_encoding_support(va_display_);
}

bool VaapiDisplayDrm::SupportsH265Encode() const {
  return check_h265_encoding_support(va_display_);
}

bool VaapiDisplayDrm::isOpen() const {
  return va_display_ != nullptr;
}

void VaapiDisplayDrm::Close() {
  if (va_display_) {
    if (drm_fd_ < 0)
      return;

    close(drm_fd_);
    drm_fd_ = -1;
    va_display_ = nullptr;
  }
}

}  // namespace livekit_ffi
