//
// Created by Théo Monnom on 31/08/2022.
//

#include "livekit/media_stream.h"

#include "libwebrtc-sys/src/media_stream.rs.h"

namespace livekit {

MediaStreamTrack::MediaStreamTrack(
    rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track)
    : track_(std::move(track)) {}

std::unique_ptr<MediaStreamTrack> MediaStreamTrack::from(
    rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track) {
  if (track->kind() == webrtc::MediaStreamTrackInterface::kVideoKind) {
    return std::make_unique<VideoTrack>(
        rtc::scoped_refptr<webrtc::VideoTrackInterface>(
            static_cast<webrtc::VideoTrackInterface*>(track.get())));
  } else {
    return std::make_unique<AudioTrack>(
        rtc::scoped_refptr<webrtc::AudioTrackInterface>(
            static_cast<webrtc::AudioTrackInterface*>(track.get())));
  }
}

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

VideoTrack::VideoTrack(rtc::scoped_refptr<webrtc::VideoTrackInterface> track)
    : MediaStreamTrack(std::move(track)) {}

void VideoTrack::add_sink(NativeVideoFrameSink& sink) {
  track()->AddOrUpdateSink(&sink, rtc::VideoSinkWants());
}

void VideoTrack::remove_sink(NativeVideoFrameSink& sink) {
  track()->RemoveSink(&sink);
}

void VideoTrack::set_should_receive(bool should_receive) {
  track()->set_should_receive(should_receive);
}

bool VideoTrack::should_receive() const {
  return track()->should_receive();
}

ContentHint VideoTrack::content_hint() const {
  return static_cast<ContentHint>(track()->content_hint());
}

void VideoTrack::set_content_hint(ContentHint hint) {
  track()->set_content_hint(
      static_cast<webrtc::VideoTrackInterface::ContentHint>(hint));
}

NativeVideoFrameSink::NativeVideoFrameSink(
    rust::Box<VideoFrameSinkWrapper> observer)
    : observer_(std::move(observer)) {}

void NativeVideoFrameSink::OnFrame(const webrtc::VideoFrame& frame) {
  observer_->on_frame(std::make_unique<VideoFrame>(frame));
}

void NativeVideoFrameSink::OnDiscardedFrame() {
  observer_->on_discarded_frame();
}

void NativeVideoFrameSink::OnConstraintsChanged(
    const webrtc::VideoTrackSourceConstraints& constraints) {
  VideoTrackSourceConstraints cst;
  cst.min_fps = constraints.min_fps.value_or(-1);
  cst.max_fps = constraints.max_fps.value_or(-1);
  observer_->on_constraints_changed(cst);
}

std::unique_ptr<NativeVideoFrameSink> create_native_video_frame_sink(
    rust::Box<VideoFrameSinkWrapper> observer) {
  return std::make_unique<NativeVideoFrameSink>(std::move(observer));
}

}  // namespace livekit
