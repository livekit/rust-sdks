//
// Created by Théo Monnom on 31/08/2022.
//

#ifndef CLIENT_SDK_NATIVE_MEDIA_STREAM_INTERFACE_H
#define CLIENT_SDK_NATIVE_MEDIA_STREAM_INTERFACE_H

#include <memory>

#include "api/media_stream_interface.h"
#include "livekit/rust_types.h"
#include "rust/cxx.h"

namespace livekit {

class MediaStreamTrack {
 public:
  explicit MediaStreamTrack(
      rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track);

  rust::String kind() const;
  rust::String id() const;

  bool enabled() const;
  bool set_enabled(bool enable);

  TrackState state() const;

 private:
  rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track_;
};

static std::unique_ptr<MediaStreamTrack> _unique_media_stream_track() {
  return nullptr;  // Ignore
}

class MediaStream {
 public:
  explicit MediaStream(rtc::scoped_refptr<webrtc::MediaStreamInterface> stream);

  rust::String id() const;

 private:
  rtc::scoped_refptr<webrtc::MediaStreamInterface> media_stream_;
};

static std::unique_ptr<MediaStream> _unique_media_stream() {
  return nullptr;  // Ignore
}
}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_MEDIA_STREAM_INTERFACE_H
