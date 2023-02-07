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

class NativeVideoFrameSink;

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

class MediaStreamTrack {
 protected:
  explicit MediaStreamTrack(
      rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track);

 public:
  static std::unique_ptr<MediaStreamTrack> from(
      rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track);

  rust::String kind() const;
  rust::String id() const;

  bool enabled() const;
  bool set_enabled(bool enable) const;

  TrackState state() const;

  rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> get() const {
    return track_;
  }

 protected:
  rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track_;
};

static std::unique_ptr<MediaStreamTrack> _unique_media_stream_track() {
  return nullptr;  // Ignore
}

class AudioTrack : public MediaStreamTrack {
 public:
  explicit AudioTrack(rtc::scoped_refptr<webrtc::AudioTrackInterface> track);
};

static std::unique_ptr<AudioTrack> _unique_audio_track() {
  return nullptr;  // Ignore
}

class VideoTrack : public MediaStreamTrack {
 public:
  explicit VideoTrack(rtc::scoped_refptr<webrtc::VideoTrackInterface> track);

  void add_sink(NativeVideoFrameSink& sink) const;
  void remove_sink(NativeVideoFrameSink& sink) const;

  void set_should_receive(bool should_receive) const;
  bool should_receive() const;
  ContentHint content_hint() const;
  void set_content_hint(ContentHint hint) const;

 private:
  webrtc::VideoTrackInterface* track() const {
    return static_cast<webrtc::VideoTrackInterface*>(track_.get());
  }
};

static std::unique_ptr<VideoTrack> _unique_video_track() {
  return nullptr;  // Ignore
}

class NativeVideoFrameSink
    : public rtc::VideoSinkInterface<webrtc::VideoFrame> {
 public:
  explicit NativeVideoFrameSink(rust::Box<VideoFrameSinkWrapper> observer);

  void OnFrame(const webrtc::VideoFrame& frame) override;
  void OnDiscardedFrame() override;
  void OnConstraintsChanged(
      const webrtc::VideoTrackSourceConstraints& constraints) override;

 private:
  rust::Box<VideoFrameSinkWrapper> observer_;
};

std::unique_ptr<NativeVideoFrameSink> create_native_video_frame_sink(
    rust::Box<VideoFrameSinkWrapper> observer);

static const MediaStreamTrack* video_to_media(const VideoTrack* track) {
  return track;
}

static const MediaStreamTrack* audio_to_media(const AudioTrack* track) {
  return track;
}

static const VideoTrack* media_to_video(const MediaStreamTrack* track) {
  return static_cast<const VideoTrack*>(track);
}

static const AudioTrack* media_to_audio(const MediaStreamTrack* track) {
  return static_cast<const AudioTrack*>(track);
}

}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_MEDIA_STREAM_INTERFACE_H
