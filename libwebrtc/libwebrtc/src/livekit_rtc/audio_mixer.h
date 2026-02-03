#ifndef LIVEKIT_AUDIO_MIXER_H
#define LIVEKIT_AUDIO_MIXER_H

#include <memory>

#include "livekit_rtc/include/capi.h"
#include "api/audio/audio_mixer.h"
#include "api/scoped_refptr.h"
#include "livekit_rtc/include/capi.h"
#include "modules/audio_mixer/audio_mixer_impl.h"
#include "modules/audio_processing/audio_buffer.h"
#include "rtc_base/synchronization/mutex.h"

namespace livekit_ffi {

class NativeAudioFrame {
 public:
  NativeAudioFrame(webrtc::AudioFrame* frame) : frame_(frame) {}
  void update_frame(uint32_t timestamp,
                    const int16_t* data,
                    size_t samples_per_channel,
                    int sample_rate_hz,
                    size_t num_channels);

 private:
  webrtc::AudioFrame* frame_;
};

class AudioMixerSource : public webrtc::AudioMixer::Source {
 public:
  AudioMixerSource(const lkAudioMixerSourceCallback* source, void* userdata);

  AudioFrameInfo GetAudioFrameWithInfo(int sample_rate_hz,
                                       webrtc::AudioFrame* audio_frame) override;

  int Ssrc() const override;

  int PreferredSampleRate() const override;

  ~AudioMixerSource() {}

 private:
  const lkAudioMixerSourceCallback* source_;
  void* userdata_;
};

class AudioMixer {
 public:
  AudioMixer();

  void add_source(const lkAudioMixerSourceCallback* source, void* userdata);

  void remove_source(int ssrc);

  size_t mix(size_t num_channels);
  const int16_t* data() const;

 private:
  mutable webrtc::Mutex sources_mutex_;
  webrtc::AudioFrame frame_;
  std::vector<std::shared_ptr<AudioMixerSource>> sources_;
  rtc::scoped_refptr<webrtc::AudioMixer> audio_mixer_;
};

std::unique_ptr<AudioMixer> create_audio_mixer();

}  // namespace livekit_ffi
<<<<<<< HEAD:libwebrtc/libwebrtc/src/livekit_rtc/audio_mixer.h

#endif  // LIVEKIT_AUDIO_MIXER_H
=======
>>>>>>> main:webrtc-sys/include/livekit/audio_mixer.h
