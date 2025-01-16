#include "vpl_session.h"

#include <rtc_base/logging.h>

// Intel VPL
#include <mfxdispatcher.h>
#include <mfxvideo.h>
    #include "va/va.h"
    #include "va/va_drm.h"
    #include <fcntl.h>
namespace sora {

struct VplSessionImpl : VplSession {
  ~VplSessionImpl();

  mfxLoader loader = nullptr;
  mfxSession session = nullptr;
};

VplSessionImpl::~VplSessionImpl() {
  MFXClose(session);
  MFXUnload(loader);
}

// Shows implementation info with Intel® VPL
void ShowImplementationInfo(mfxLoader loader, mfxU32 implnum) {
    mfxImplDescription *idesc = nullptr;
    mfxStatus sts;
    //Loads info about implementation at specified list location
    sts = MFXEnumImplementations(loader, implnum, MFX_IMPLCAPS_IMPLDESCSTRUCTURE, (mfxHDL *)&idesc);
    if (!idesc || (sts != MFX_ERR_NONE))
        return;

    printf("Implementation details:\n");
    printf("  ApiVersion:           %hu.%hu  \n", idesc->ApiVersion.Major, idesc->ApiVersion.Minor);
    printf("  Implementation type: HW\n");
    printf("  AccelerationMode via: ");
    switch (idesc->AccelerationMode) {
        case MFX_ACCEL_MODE_NA:
            printf("NA \n");
            break;
        case MFX_ACCEL_MODE_VIA_D3D9:
            printf("D3D9\n");
            break;
        case MFX_ACCEL_MODE_VIA_D3D11:
            printf("D3D11\n");
            break;
        case MFX_ACCEL_MODE_VIA_VAAPI:
            printf("VAAPI\n");
            break;
        case MFX_ACCEL_MODE_VIA_VAAPI_DRM_MODESET:
            printf("VAAPI_DRM_MODESET\n");
            break;
        case MFX_ACCEL_MODE_VIA_VAAPI_GLX:
            printf("VAAPI_GLX\n");
            break;
        case MFX_ACCEL_MODE_VIA_VAAPI_X11:
            printf("VAAPI_X11\n");
            break;
        case MFX_ACCEL_MODE_VIA_VAAPI_WAYLAND:
            printf("VAAPI_WAYLAND\n");
            break;
        case MFX_ACCEL_MODE_VIA_HDDLUNITE:
            printf("HDDLUNITE\n");
            break;
        default:
            printf("unknown\n");
            break;
    }
    printf("  DeviceID:             %s \n", idesc->Dev.DeviceID);
    MFXDispReleaseImplDescription(loader, idesc);

#if (MFX_VERSION >= 2004)
    //Show implementation path, added in 2.4 API
    mfxHDL implPath = nullptr;
    sts             = MFXEnumImplementations(loader, implnum, MFX_IMPLCAPS_IMPLPATH, &implPath);
    if (!implPath || (sts != MFX_ERR_NONE))
        return;

    printf("  Path: %s\n\n", reinterpret_cast<mfxChar *>(implPath));
    MFXDispReleaseImplDescription(loader, implPath);
#endif
}

void *InitAcceleratorHandle(mfxSession session, int *fd, mfxIMPL impl) {
    // printf("in init accel\n");
    // mfxIMPL impl;
    // mfxStatus sts = MFXQueryIMPL(session, &impl);
    // if (sts != MFX_ERR_NONE)
    //     return NULL;

// #ifdef LIBVA_SUPPORT
    printf("in libva support\n");
    if ((impl & MFX_IMPL_VIA_VAAPI) == MFX_IMPL_VIA_VAAPI) {
        if (!fd)
            return NULL;
        VADisplay va_dpy = NULL;
        // initialize VAAPI context and set session handle (req in Linux)
        *fd = open("/dev/dri/renderD128", O_RDWR);
        if (*fd >= 0) {
            va_dpy = vaGetDisplayDRM(*fd);
            if (va_dpy) {
                int major_version = 0, minor_version = 0;
                if (VA_STATUS_SUCCESS == vaInitialize(va_dpy, &major_version, &minor_version)) {
                    MFXVideoCORE_SetHandle(session,
                                           static_cast<mfxHandleType>(MFX_HANDLE_VA_DISPLAY),
                                           va_dpy);
                }
            }
        }
        return va_dpy;
    }
// #endif

    return NULL;
}

std::shared_ptr<VplSession> VplSession::Create() {
  std::shared_ptr<VplSessionImpl> session(new VplSessionImpl());

  mfxStatus sts = MFX_ERR_NONE;

  session->loader = MFXLoad();
  if (session->loader == nullptr) {
    RTC_LOG(LS_VERBOSE) << "Failed to MFXLoad";
    return nullptr;
  }

  MFX_ADD_PROPERTY_U32(session->loader, "mfxImplDescription.Impl",
                       MFX_IMPL_TYPE_HARDWARE);

  sts = MFXCreateSession(session->loader, 0, &session->session);
  if (sts != MFX_ERR_NONE) {
    RTC_LOG(LS_VERBOSE) << "Failed to MFXCreateSession: sts=" << sts;
    return nullptr;
  }


  // Query selected implementation and version
  mfxIMPL impl;
  sts = MFXQueryIMPL(session->session, &impl);
  if (sts != MFX_ERR_NONE) {
    RTC_LOG(LS_VERBOSE) << "Failed to MFXQueryIMPL: sts=" << sts;
    return nullptr;
  }

    int accel_fd = 0;
  InitAcceleratorHandle(session->session, &accel_fd, impl);

  mfxVersion ver;
  sts = MFXQueryVersion(session->session, &ver);
  if (sts != MFX_ERR_NONE) {
    RTC_LOG(LS_VERBOSE) << "Failed to MFXQueryVersion: sts=" << sts;
    return nullptr;
  }

  RTC_LOG(LS_VERBOSE) << "Intel VPL Implementation: "
                      << (impl == MFX_IMPL_SOFTWARE ? "SOFTWARE" : "HARDWARE");
  RTC_LOG(LS_VERBOSE) << "Intel VPL Version: " << ver.Major << "." << ver.Minor;
  ShowImplementationInfo(session->loader, 0);

  return session;
}

mfxSession GetVplSession(std::shared_ptr<VplSession> session) {
  return std::static_pointer_cast<VplSessionImpl>(session)->session;
}

} // namespace sora
