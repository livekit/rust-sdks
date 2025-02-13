#include "vpl_session.h"

#include <rtc_base/logging.h>

#include <fcntl.h>
#include <mfxvideo.h>
#include "va/va.h"
#include "va/va_drm.h"

namespace {
constexpr char* GPU_RENDER_NODE = "/dev/dri/renderD128";
}

namespace any_vpl {

VplSession::VplSession() {
  Create();
}

VplSession::~VplSession() {
  MFXClose(session_);
  MFXUnload(loader_);
}

mfxSession VplSession::GetSession() const {
  return session_;
}

bool VplSession::Create() {
  mfxStatus sts = MFX_ERR_NONE;

  loader_ = MFXLoad();
  if (loader_ == nullptr) {
    RTC_LOG(LS_ERROR) << "MFXLoad failed";
    return false;
  }

  constexpr char* implementationDescription = "mfxImplDescription.Impl";
  MFX_ADD_PROPERTY_U32(loader_, implementationDescription, MFX_IMPL_TYPE_HARDWARE);

  sts = MFXCreateSession(loader_, 0, &session_);
  if (sts != MFX_ERR_NONE) {
    RTC_LOG(LS_ERROR) << "MFXCreateSession failed: sts=" << sts;
    return false;
  }

  // Query selected implementation
  mfxIMPL implementation;
  sts = MFXQueryIMPL(session_, &implementation);
  if (sts != MFX_ERR_NONE) {
    RTC_LOG(LS_ERROR) << "MFXQueryIMPL failed: sts=" << sts;
    return false;
  }

  InitAcceleratorHandle(implementation);

  mfxVersion version;
  sts = MFXQueryVersion(session_, &version);
  if (sts != MFX_ERR_NONE) {
    RTC_LOG(LS_ERROR) << "MFXQueryVersion failed: sts=" << sts;
    return false;
  }

  RTC_LOG(LS_INFO) << "Intel VPL Implementation: " << (implementation == MFX_IMPL_SOFTWARE ? "SOFTWARE" : "HARDWARE");
  RTC_LOG(LS_INFO) << "Intel VPL Version: " << version.Major << "." << version.Minor;
  ShowImplementationInfo(0);

  return true;
}

void VplSession::ShowImplementationInfo(mfxU32 implnum) {
  mfxImplDescription* idesc = nullptr;
  mfxStatus sts;
  // Loads info about implementation at specified list location
  sts = MFXEnumImplementations(loader_, implnum, MFX_IMPLCAPS_IMPLDESCSTRUCTURE, (mfxHDL*)&idesc);
  if (!idesc || (sts != MFX_ERR_NONE)) {
    return;
  }

  RTC_LOG(LS_INFO) << "Implementation details:\n";
  RTC_LOG(LS_INFO) << "  ApiVersion: " << idesc->ApiVersion.Major << "." << idesc->ApiVersion.Minor;
  RTC_LOG(LS_INFO) << "  AccelerationMode via: ";
  switch (idesc->AccelerationMode) {
    case MFX_ACCEL_MODE_NA:
      RTC_LOG(LS_INFO) << "NA";
      break;
    case MFX_ACCEL_MODE_VIA_D3D9:
      RTC_LOG(LS_INFO) << "D3D9";
      break;
    case MFX_ACCEL_MODE_VIA_D3D11:
      RTC_LOG(LS_INFO) << "D3D11";
      break;
    case MFX_ACCEL_MODE_VIA_VAAPI:
      RTC_LOG(LS_INFO) << "VAAPI";
      break;
    case MFX_ACCEL_MODE_VIA_VAAPI_DRM_MODESET:
      RTC_LOG(LS_INFO) << "VAAPI_DRM_MODESET";
      break;
    case MFX_ACCEL_MODE_VIA_VAAPI_GLX:
      RTC_LOG(LS_INFO) << "VAAPI_GLX";
      break;
    case MFX_ACCEL_MODE_VIA_VAAPI_X11:
      RTC_LOG(LS_INFO) << "VAAPI_X11";
      break;
    case MFX_ACCEL_MODE_VIA_VAAPI_WAYLAND:
      RTC_LOG(LS_INFO) << "VAAPI_WAYLAND";
      break;
    case MFX_ACCEL_MODE_VIA_HDDLUNITE:
      RTC_LOG(LS_INFO) << "HDDLUNITE";
      break;
    default:
      RTC_LOG(LS_INFO) << "unknown";
      break;
  }
  RTC_LOG(LS_INFO) << "  DeviceID: " << idesc->Dev.DeviceID;
  MFXDispReleaseImplDescription(loader_, idesc);

#if (MFX_VERSION >= 2004)
  // Show implementation path, added in 2.4 API
  mfxHDL implPath = nullptr;
  sts = MFXEnumImplementations(loader_, implnum, MFX_IMPLCAPS_IMPLPATH, &implPath);
  if (!implPath || (sts != MFX_ERR_NONE)) {
    return;
  }

  RTC_LOG(LS_INFO) << "  Path: " << reinterpret_cast<mfxChar*>(implPath);
  MFXDispReleaseImplDescription(loader_, implPath);
#endif
}

void VplSession::InitAcceleratorHandle(mfxIMPL implementation) {
  if ((implementation & MFX_IMPL_VIA_VAAPI) != MFX_IMPL_VIA_VAAPI) {
    return;
  }
  // initialize VAAPI context and set session handle (req in Linux)
  accelratorFD_ = open(GPU_RENDER_NODE, O_RDWR);
  if (accelratorFD_ < 0) {
    RTC_LOG(LS_ERROR) << "Failed to open GPU render node: " << GPU_RENDER_NODE;
    return;
  }

  vaDisplay_ = vaGetDisplayDRM(accelratorFD_);
  if (!vaDisplay_) {
    RTC_LOG(LS_ERROR) << "Failed to get VA display from GPU render node: " << GPU_RENDER_NODE;
    return;
  }

  int majorVersion = 0, minorVersion = 0;
  if (VA_STATUS_SUCCESS != vaInitialize(vaDisplay_, &majorVersion, &minorVersion)) {
    RTC_LOG(LS_ERROR) << "Failed to initialize VA library";
    return;
  }

  RTC_LOG(LS_INFO) << "VAAPI initialized. Version: " << majorVersion << "." << minorVersion;
  if (MFXVideoCORE_SetHandle(session_, static_cast<mfxHandleType>(MFX_HANDLE_VA_DISPLAY), vaDisplay_) != MFX_ERR_NONE) {
    RTC_LOG(LS_ERROR) << "Failed to set VA display handle for the VA library to use";
  }
}

}  // namespace any_vpl
