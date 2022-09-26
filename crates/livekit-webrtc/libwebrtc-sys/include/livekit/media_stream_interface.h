//
// Created by Théo Monnom on 31/08/2022.
//

#ifndef CLIENT_SDK_NATIVE_MEDIA_STREAM_INTERFACE_H
#define CLIENT_SDK_NATIVE_MEDIA_STREAM_INTERFACE_H

#include <memory>

#include "api/media_stream_interface.h"

namespace livekit {

class MediaStreamInterface {
 public:
  explicit MediaStreamInterface(
      rtc::scoped_refptr<webrtc::MediaStreamInterface> stream);

 private:
  rtc::scoped_refptr<webrtc::MediaStreamInterface> media_stream_;
};

static std::unique_ptr<MediaStreamInterface> _unique_media_stream() {
  return nullptr;  // Ignore
}
}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_MEDIA_STREAM_INTERFACE_H
