//
// Created by Théo Monnom on 31/08/2022.
//

#include "livekit/media_stream.h"

namespace livekit {

MediaStreamTrack::MediaStreamTrack(
    rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track)
    : track_(std::move(track)) {}

rust::String MediaStreamTrack::kind() const {
  return track_->kind();
}

rust::String MediaStreamTrack::id() const {
  return track_->id();
}

bool MediaStreamTrack::enabled() const {
  return track_->enabled();
}

bool MediaStreamTrack::set_enabled(bool enable) {
  return track_->set_enabled(enable);
}

TrackState MediaStreamTrack::state() const {
  return static_cast<TrackState>(track_->state());
}

MediaStream::MediaStream(
    rtc::scoped_refptr<webrtc::MediaStreamInterface> stream)
    : media_stream_(std::move(stream)) {}

rust::String MediaStream::id() const {
    return media_stream_->id();
}

}  // namespace livekit