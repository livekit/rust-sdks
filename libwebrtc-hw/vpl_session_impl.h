#ifndef SORA_HWENC_VPL_VPL_SESSION_IMPL_H_
#define SORA_HWENC_VPL_VPL_SESSION_IMPL_H_

#include <iostream>

// Intel VPL
#include <mfxvideo++.h>

#include "vpl_session.h"

namespace sora {

mfxSession GetVplSession(std::shared_ptr<VplSession> session);

} // namespace sora

#endif