#include "vaapi_display.h"

#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

#include "vaapi_encoder.h"
#ifdef IN_LIBVA
#include "va/drm/va_drm.h"
#else
#include <va/va_drm.h>
#endif

static VADisplay va_open_display_drm(int* drm_fd) {
  VADisplay va_dpy;
  int i;

  static const char* drm_device_paths[] = {
      "/dev/dri/renderD128", "/dev/dri/card0", "/dev/dri/renderD129",
      "/dev/dri/card1", NULL};
  for (i = 0; drm_device_paths[i]; i++) {
    *drm_fd = open(drm_device_paths[i], O_RDWR);
    if (*drm_fd < 0)
      continue;

    va_dpy = vaGetDisplayDRM(*drm_fd);
    if (va_dpy)
      return va_dpy;

    close(*drm_fd);
    *drm_fd = -1;
  }
  return NULL;
}

namespace livekit {

bool VaapiDisplayDrm::Open() {
  va_display_ = va_open_display_drm(&drm_fd_);
  if (!va_display_) {
    fprintf(stderr, "Failed to open VA display\n");
    return false;
  }
  return true;
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

}  // namespace livekit
