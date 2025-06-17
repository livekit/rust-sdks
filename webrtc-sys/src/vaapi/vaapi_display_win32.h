#ifndef VAAPI_DISPLAY_WIN32_H_
#define VAAPI_DISPLAY_WIN32_H_

#include <stdio.h>

#include <va/va_win32.h>

namespace livekit {

// VAAPI win32 display wrapper class
class VaapiDisplayWin32 {
 public:
  VaapiDisplayWin32() = default;
  VaapiDisplayDrm(const VaapiDisplayWin32&) = delete;
  ~VaapiDisplayWin32() = default;

  // Initialize the VAAPI display
  bool Open();

  // Check if the VAAPI display is open
  bool isOpen() const;
  
  // Close the VAAPI display
  void Close();

  // Get the VAAPI display handle
  VADisplay display() const { return va_display_; }

 private:
  VADisplay va_display_;
  int drm_fd_;
};

}  // namespace livekit

#endif  // VAAPI_DISPLAY_WIN32_H_
