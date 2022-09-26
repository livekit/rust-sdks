//
// Created by Théo Monnom on 31/08/2022.
//

#include "livekit/media_stream_interface.h"

namespace livekit {

MediaStreamInterface::MediaStreamInterface(
    rtc::scoped_refptr<webrtc::MediaStreamInterface> stream)
    : media_stream_(std::move(stream)) {}
}  // namespace livekit