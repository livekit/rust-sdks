//
// Created by Théo Monnom on 31/08/2022.
//

#ifndef CLIENT_SDK_NATIVE_MEDIA_STREAM_INTERFACE_H
#define CLIENT_SDK_NATIVE_MEDIA_STREAM_INTERFACE_H

#include <memory>

#include "api/media_stream_interface.h"
#include "api/video/video_frame.h"
#include "livekit/rust_types.h"
#include "livekit/video_frame.h"
#include "media/base/adapted_video_track_source.h"
#include "rtc_base/synchronization/mutex.h"
#include "rtc_base/timestamp_aligner.h"
#include "rust/cxx.h"

namespace livekit {

class NativeVideoFrameSink;
class VideoTrack;
class AudioTrack;

class MediaStream {
 public:
  explicit MediaStream(rtc::scoped_refptr<webrtc::MediaStreamInterface> stream);

  rust::String id() const;
  rust::Vec<std::shared_ptr<VideoTrack>> get_video_tracks() const;
  rust::Vec<std::shared_ptr<AudioTrack>> get_audio_tracks() const;

  std::shared_ptr<AudioTrack> find_audio_track(rust::String track_id) const;
  std::shared_ptr<VideoTrack> find_video_track(rust::String track_id) const;

  bool add_audio_track(std::shared_ptr<AudioTrack> audio_track) const;
  bool add_video_track(std::shared_ptr<VideoTrack> video_track) const;
  bool remove_audio_track(std::shared_ptr<AudioTrack> audio_track) const;
  bool remove_video_track(std::shared_ptr<VideoTrack> video_track) const;

 private:
  rtc::scoped_refptr<webrtc::MediaStreamInterface> media_stream_;
};

class MediaStreamTrack {
 protected:
  explicit MediaStreamTrack(
      rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track);

 public:
  static std::shared_ptr<MediaStreamTrack> from(
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

class AudioTrack : public MediaStreamTrack {
 public:
  explicit AudioTrack(rtc::scoped_refptr<webrtc::AudioTrackInterface> track);
};

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

// Native impl of the WebRTC interface
class NativeVideoTrackSource : public rtc::AdaptedVideoTrackSource {
 public:
  NativeVideoTrackSource();
  ~NativeVideoTrackSource() override;

  bool is_screencast() const override;
  absl::optional<bool> needs_denoising() const override;
  webrtc::MediaSourceInterface::SourceState state() const override;
  bool remote() const override;

  bool on_captured_frame(const webrtc::VideoFrame& frame);

 private:
  webrtc::Mutex mutex_;
  rtc::TimestampAligner timestamp_aligner_;
};

class AdaptedVideoTrackSource {
 public:
  AdaptedVideoTrackSource(rtc::scoped_refptr<NativeVideoTrackSource> source);

  bool on_captured_frame(std::unique_ptr<VideoFrame> frame)
      const;  // frames pushed from Rust (+interior mutability)

  rtc::scoped_refptr<NativeVideoTrackSource> get() const;

 private:
  rtc::scoped_refptr<NativeVideoTrackSource> source_;
};

std::unique_ptr<AdaptedVideoTrackSource> create_adapted_video_track_source();

static const VideoTrack* media_to_video(const MediaStreamTrack* track) {
  return static_cast<const VideoTrack*>(track);
}

static const AudioTrack* media_to_audio(const MediaStreamTrack* track) {
  return static_cast<const AudioTrack*>(track);
}

static std::shared_ptr<MediaStreamTrack> _shared_media_stream_track() {
  return nullptr;  // Ignore
}

static std::shared_ptr<AudioTrack> _shared_audio_track() {
  return nullptr;  // Ignore
}

static std::shared_ptr<VideoTrack> _shared_video_track() {
  return nullptr;  // Ignore
}

static std::shared_ptr<MediaStream> _shared_media_stream() {
  return nullptr;  // Ignore
}

}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_MEDIA_STREAM_INTERFACE_H
