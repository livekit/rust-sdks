#include "livekit_rtc/capi.h"

#include "api/make_ref_counted.h"
#include "livekit_rtc/audio_track.h"
#include "livekit_rtc/data_channel.h"
#include "livekit_rtc/ice_candidate.h"
#include "livekit_rtc/media_stream.h"
#include "livekit_rtc/media_stream_track.h"
#include "livekit_rtc/peer.h"
#include "livekit_rtc/session_description.h"
#include "livekit_rtc/video_frame_buffer.h"
#include "livekit_rtc/video_track.h"
#include "rtc_base/logging.h"
#include "rtc_base/ref_count.h"
#include "rtc_base/ssl_adapter.h"

void lkAddRef(lkRefCountedObject* rc) {
  reinterpret_cast<webrtc::RefCountInterface*>(rc)->AddRef();
}

void lkReleaseRef(lkRefCountedObject* rc) {
  reinterpret_cast<webrtc::RefCountInterface*>(rc)->Release();
}

int lkInitialize() {
  if (!webrtc::InitializeSSL()) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to InitializeSSL()";
    return 0;
  }

#ifdef WEBRTC_WIN
  WSADATA data;
  WSAStartup(MAKEWORD(1, 0), &data);
#endif

  return 1;
}

int lkDispose() {
  if (!webrtc::CleanupSSL()) {
    RTC_LOG_ERR(LS_ERROR) << "Failed to CleanupSSL()";
    return 0;
  }

#ifdef WEBRTC_WIN
  WSACleanup();
#endif

  return 1;
}

lkPeerFactory* lkCreatePeerFactory() {
  return reinterpret_cast<lkPeerFactory*>(
      webrtc::make_ref_counted<livekit::PeerFactory>().release());
}

lkPeer* lkCreatePeer(lkPeerFactory* factory,
                     const lkRtcConfiguration* config,
                     const lkPeerObserver* observer,
                     void* userdata) {
  return reinterpret_cast<lkPeer*>(
      reinterpret_cast<livekit::PeerFactory*>(factory)
          ->CreatePeer(config, observer, userdata)
          .release());
}

lkDataChannel* lkCreateDataChannel(lkPeer* peer,
                                   const char* label,
                                   const lkDataChannelInit* init) {
  return reinterpret_cast<lkDataChannel*>(reinterpret_cast<livekit::Peer*>(peer)
                                              ->CreateDataChannel(label, init)
                                              .release());
}

bool lkAddIceCandidate(lkPeer* peer,
                       lkIceCandidate* candidate,
                       void (*onComplete)(lkRtcError* error, void* userdata),
                       void* userdata) {
  return reinterpret_cast<livekit::Peer*>(peer)->AddIceCandidate(
      candidate, onComplete, userdata);
}

bool lkSetLocalDescription(lkPeer* peer,
                           const lkSessionDescription* desc,
                           const lkSetSdpObserver* observer,
                           void* userdata) {
  return reinterpret_cast<livekit::Peer*>(peer)->SetLocalDescription(
      desc, observer, userdata);
}

bool lkSetRemoteDescription(lkPeer* peer,
                            const lkSessionDescription* desc,
                            const lkSetSdpObserver* observer,
                            void* userdata) {
  return reinterpret_cast<livekit::Peer*>(peer)->SetRemoteDescription(
      desc, observer, userdata);
}

bool lkCreateOffer(lkPeer* peer,
                   const lkOfferAnswerOptions* options,
                   const lkCreateSdpObserver* observer,
                   void* userdata) {
  return reinterpret_cast<livekit::Peer*>(peer)->CreateOffer(*options, observer,
                                                             userdata);
}

bool lkCreateAnswer(lkPeer* peer,
                    const lkOfferAnswerOptions* options,
                    const lkCreateSdpObserver* observer,
                    void* userdata) {
  return reinterpret_cast<livekit::Peer*>(peer)->CreateAnswer(
      *options, observer, userdata);
}

bool lkPeerSetConfig(lkPeer* peer, const lkRtcConfiguration* config) {
  return reinterpret_cast<livekit::Peer*>(peer)->SetConfig(config);
}

bool lkPeerClose(lkPeer* peer) {
  return reinterpret_cast<livekit::Peer*>(peer)->Close();
}

void lkDcRegisterObserver(lkDataChannel* dc,
                          const lkDataChannelObserver* observer,
                          void* userdata) {
  reinterpret_cast<livekit::DataChannel*>(dc)->RegisterObserver(observer,
                                                                userdata);
}

void lkDcUnregisterObserver(lkDataChannel* dc) {
  reinterpret_cast<livekit::DataChannel*>(dc)->UnregisterObserver();
}

lkDcState lkDcGetState(lkDataChannel* dc) {
  return reinterpret_cast<livekit::DataChannel*>(dc)->State();
}

int lkDcGetId(lkDataChannel* dc) {
  return reinterpret_cast<livekit::DataChannel*>(dc)->Id();
}

int lkDcGetLabelLength(lkDataChannel* dc) {
  auto label = reinterpret_cast<livekit::DataChannel*>(dc)->label();
  return static_cast<int>(label.size());
}

int lkDcGetLabel(lkDataChannel* dc, char* buffer, int bufferSize) {
  auto label = reinterpret_cast<livekit::DataChannel*>(dc)->label();
  int len = static_cast<int>(label.size());
  if (bufferSize > 0) {
    int copySize = (len < bufferSize) ? len : bufferSize;
    memcpy(buffer, label.c_str(), copySize);
  }
  return len;
}

uint64_t lkDcGetBufferedAmount(lkDataChannel* dc) {
  return reinterpret_cast<livekit::DataChannel*>(dc)->buffered_amount();
}

void lkDcSendAsync(lkDataChannel* dc,
                   const uint8_t* data,
                   uint64_t size,
                   bool binary,
                   void (*onComplete)(lkRtcError* error, void* userdata),
                   void* userdata) {
  reinterpret_cast<livekit::DataChannel*>(dc)->SendAsync(data, size, binary,
                                                         onComplete, userdata);
}

void lkDcClose(lkDataChannel* dc) {
  reinterpret_cast<livekit::DataChannel*>(dc)->Close();
}

lkSessionDescription* lkCreateSessionDescription(lkSdpType type,
                                                 const char* sdp) {
  auto desc = livekit::SessionDescription::Create(
      std::string(sdp), static_cast<webrtc::SdpType>(type));
  if (!desc) {
    return nullptr;
  }
  return reinterpret_cast<lkSessionDescription*>(desc.release());
}

lkSdpType lkSessionDescriptionGetType(lkSessionDescription* desc) {
  return static_cast<lkSdpType>(
      reinterpret_cast<livekit::SessionDescription*>(desc)->GetType());
}

int lkSessionDescriptionGetSdpLength(lkSessionDescription* desc) {
  std::string sdp =
      reinterpret_cast<livekit::SessionDescription*>(desc)->ToString();
  return sdp.length();
}

int lkSessionDescriptionGetSdp(lkSessionDescription* desc,
                               char* buffer,
                               int bufferSize) {
  std::string sdp =
      reinterpret_cast<livekit::SessionDescription*>(desc)->ToString();
  int len = static_cast<int>(sdp.size());
  if (bufferSize > 0) {
    int copySize = (len < bufferSize) ? len : bufferSize;
    memcpy(buffer, sdp.c_str(), copySize);
  }
  return len;
}

lkIceCandidate* lkCreateIceCandidate(const char* mid,
                                     int mlineIndex,
                                     const char* sdp) {
  auto candidate = livekit::IceCandidate::Create(std::string(mid), mlineIndex,
                                                 std::string(sdp));
  if (!candidate) {
    return nullptr;
  }
  return reinterpret_cast<lkIceCandidate*>(candidate.release());
}

int lkIceCandidateGetMlineIndex(lkIceCandidate* candidate) {
  return reinterpret_cast<livekit::IceCandidate*>(candidate)->mline_index();
}

int lkIceCandidateGetMidLength(lkIceCandidate* candidate) {
  auto mid = reinterpret_cast<livekit::IceCandidate*>(candidate)->mid();
  return static_cast<int>(mid.size());
}

int lkIceCandidateGetMid(lkIceCandidate* candidate,
                         char* buffer,
                         int bufferSize) {
  auto mid = reinterpret_cast<livekit::IceCandidate*>(candidate)->mid();
  int len = static_cast<int>(mid.size());
  if (bufferSize > 0) {
    int copySize = (len < bufferSize) ? len : bufferSize;
    memcpy(buffer, mid.c_str(), copySize);
  }
  return len;
}

int lkIceCandidateGetSdpLength(lkIceCandidate* candidate) {
  std::string sdp = reinterpret_cast<livekit::IceCandidate*>(candidate)->sdp();
  return sdp.length();
}

int lkIceCandidateGetSdp(lkIceCandidate* candidate,
                         char* buffer,
                         int bufferSize) {
  std::string sdp = reinterpret_cast<livekit::IceCandidate*>(candidate)->sdp();
  int len = static_cast<int>(sdp.size());
  if (bufferSize > 0) {
    int copySize = (len < bufferSize) ? len : bufferSize;
    memcpy(buffer, sdp.c_str(), copySize);
  }
  return len;
}

void lkPeerRestartIce(lkPeer* peer) {
  reinterpret_cast<livekit::Peer*>(peer)->RestartIce();
}

lkPeerState lkGetPeerState(lkPeer* peer) {
  return static_cast<lkPeerState>(
      reinterpret_cast<livekit::Peer*>(peer)->GetPeerState());
}

lkIceGatheringState lkPeerGetIceGatheringState(lkPeer* peer) {
  return static_cast<lkIceGatheringState>(
      reinterpret_cast<livekit::Peer*>(peer)->GetIceGatheringState());
}

lkIceState lkPeerGetIceConnectionState(lkPeer* peer) {
  return static_cast<lkIceState>(
      reinterpret_cast<livekit::Peer*>(peer)->GetIceConnectionState());
}

lkSignalingState lkPeerGetSignalingState(lkPeer* peer) {
  return static_cast<lkSignalingState>(
      reinterpret_cast<livekit::Peer*>(peer)->GetSignalingState());
}

const lkSessionDescription* lkPeerGetCurrentLocalDescription(lkPeer* peer) {
  return reinterpret_cast<livekit::Peer*>(peer)->GetCurrentLocalDescription();
}

const lkSessionDescription* lkPeerGetCurrentRemoteDescription(lkPeer* peer) {
  return reinterpret_cast<livekit::Peer*>(peer)->GetCurrentRemoteDescription();
}

lkRtpCapabilities* lkGetRtpSenderCapabilities(lkPeerFactory* factory) {
  auto peer_factory = reinterpret_cast<livekit::PeerFactory*>(factory)
                          ->GetPeerConnectionFactory();
  return nullptr;
}

lkRtpCapabilities* lkGetRtpReceiverCapabilities(lkPeerFactory* factory) {
  auto peer_factory = reinterpret_cast<livekit::PeerFactory*>(factory)
                          ->GetPeerConnectionFactory();
  return nullptr;
}

lkRtcVideoTrack* CreateVideoTrack(const char* id, lkVideoTrackSource* source) {
  return nullptr;
}

lkRtcAudioTrack* CreateAudioTrack(const char* id, lkAudioTrackSource* source) {
  return nullptr;
}

lkNativeAudioSink* lkCreateNativeAudioSink(
    int sample_rate,
    int num_channels,
    void (*onAudioData)(int16_t* audioData,
                        uint32_t sampleRate,
                        uint32_t numberOfChannels,
                        int numberOfFrames,
                        void* userdata),
    void* userdata) {
  return reinterpret_cast<lkNativeAudioSink*>(
      webrtc::make_ref_counted<livekit::NativeAudioSink>(
          sample_rate, num_channels, onAudioData, userdata)
          .release());
}

lkAudioTrackSource* lkCreateAudioTrackSource(lkAudioSourceOptions options,
                                             int sample_rate,
                                             int num_channels,
                                             int queue_size_ms) {
  return reinterpret_cast<lkAudioTrackSource*>(
      livekit::AudioTrackSource::Create(options, sample_rate, num_channels,
                                        queue_size_ms)
          .release());
}

void lkAudioTrackSourceSetAudioOptions(lkAudioTrackSource* source,
                                       const lkAudioSourceOptions* options) {
  reinterpret_cast<livekit::AudioTrackSource*>(source)->set_audio_options(
      *options);
}

lkAudioSourceOptions lkAudioTrackSourceGetAudioOptions(
    lkAudioTrackSource* source) {
  return reinterpret_cast<livekit::AudioTrackSource*>(source)->audio_options();
}

bool lkAudioTrackSourceCaptureFrame(lkAudioTrackSource* source,
                                    const int16_t* audio_data,
                                    uint32_t sample_rate,
                                    uint32_t number_of_channels,
                                    int number_of_frames,
                                    void* userdata,
                                    void (*onComplete)(void* userdata)) {
  std::vector<int16_t> audio_vector(
      audio_data, audio_data + number_of_channels * number_of_frames);
  return reinterpret_cast<livekit::AudioTrackSource*>(source)->capture_frame(
      audio_vector, sample_rate, number_of_channels, number_of_frames, userdata,
      onComplete);
}

void lkAudioTrackSourceClearBuffer(lkAudioTrackSource* source) {
  reinterpret_cast<livekit::AudioTrackSource*>(source)->clear_buffer();
}

int lkAudioTrackSourceGetSampleRate(lkAudioTrackSource* source) {
  return reinterpret_cast<livekit::AudioTrackSource*>(source)->sample_rate();
}

int lkAudioTrackSourceGetNumChannels(lkAudioTrackSource* source) {
  return reinterpret_cast<livekit::AudioTrackSource*>(source)->num_channels();
}

int lkAudioTrackSourceAddSink(lkAudioTrackSource* source,
                              lkNativeAudioSink* sink) {
  reinterpret_cast<livekit::AudioTrackSource*>(source)->get()->AddSink(
      reinterpret_cast<livekit::NativeAudioSink*>(sink)->audio_track_sink());
  return 1;
}

int lkAudioTrackSourceRemoveSink(lkAudioTrackSource* source,
                                 lkNativeAudioSink* sink) {
  reinterpret_cast<livekit::AudioTrackSource*>(source)->get()->RemoveSink(
      reinterpret_cast<livekit::NativeAudioSink*>(sink)->audio_track_sink());
  return 1;
}

int lkMediaStreamTrackGetIdLength(lkMediaStreamTrack* track) {
  auto id = reinterpret_cast<livekit::MediaStreamTrack*>(track)->id();
  return static_cast<int>(id.size());
}

int lkMediaStreamTrackGetId(lkMediaStreamTrack* track,
                            char* buffer,
                            int bufferSize) {
  auto id = reinterpret_cast<livekit::MediaStreamTrack*>(track)->id();
  int len = static_cast<int>(id.size());
  if (bufferSize > 0) {
    int copySize = (len < bufferSize) ? len : bufferSize;
    memcpy(buffer, id.c_str(), copySize);
  }
  return len;
}

bool lkMediaStreamTrackIsEnabled(lkMediaStreamTrack* track) {
  return reinterpret_cast<livekit::MediaStreamTrack*>(track)->enabled();
}

void lkMediaStreamTrackSetEnabled(lkMediaStreamTrack* track, bool enabled) {
  reinterpret_cast<livekit::MediaStreamTrack*>(track)->set_enabled(enabled);
}

lkRtcTrackState lkMediaStreamTrackGetState(lkMediaStreamTrack* track) {
  return static_cast<lkRtcTrackState>(
      reinterpret_cast<livekit::MediaStreamTrack*>(track)->state());
}

lkMediaStreamTrackKind lkMediaStreamTrackGetKind(lkMediaStreamTrack* track) {
  auto kind = reinterpret_cast<livekit::MediaStreamTrack*>(track)->kind();
  if (kind == "audio") {
    return lkMediaStreamTrackKind::LK_MEDIA_STREAM_TRACK_KIND_AUDIO;
  } else if (kind == "video") {
    return lkMediaStreamTrackKind::LK_MEDIA_STREAM_TRACK_KIND_VIDEO;
  } else if (kind == "data") {
    return lkMediaStreamTrackKind::LK_MEDIA_STREAM_TRACK_KIND_DATA;
  } else {
    return lkMediaStreamTrackKind::LK_MEDIA_STREAM_TRACK_KIND_UNKNOWN;
  }
}

lkRtcAudioTrack* lkPeerFactoryCreateAudioTrack(lkPeerFactory* factory,
                                               const char* id,
                                               lkAudioTrackSource* source) {
  return reinterpret_cast<livekit::PeerFactory*>(factory)->CreateAudioTrack(
      id, source);
}

lkRtcVideoTrack* lkPeerFactoryCreateVideoTrack(lkPeerFactory* factory,
                                               const char* id,
                                               lkVideoTrackSource* source) {
  return reinterpret_cast<livekit::PeerFactory*>(factory)->CreateVideoTrack(
      id, source);
}

void lkAudioTrackAddSink(lkRtcAudioTrack* track, lkNativeAudioSink* sink) {
  reinterpret_cast<livekit::AudioTrack*>(track)->add_sink(
      webrtc::scoped_refptr<livekit::NativeAudioSink>(
          reinterpret_cast<livekit::NativeAudioSink*>(sink)));
}

void lkAudioTrackRemoveSink(lkRtcAudioTrack* track, lkNativeAudioSink* sink) {
  reinterpret_cast<livekit::AudioTrack*>(track)->remove_sink(
      webrtc::scoped_refptr<livekit::NativeAudioSink>(
          reinterpret_cast<livekit::NativeAudioSink*>(sink)));
}

lkRtcAudioTrack** lkMediaStreamGetAudioTracks(lkMediaStream* stream,
                                              int* trackCount) {
  auto media_stream =
      reinterpret_cast<livekit::MediaStream*>(stream)->media_stream();
  auto audio_tracks = media_stream->GetAudioTracks();
  *trackCount = static_cast<int>(audio_tracks.size());
  if (*trackCount == 0) {
    return nullptr;
  }

  lkRtcAudioTrack** track_array = new lkRtcAudioTrack*[*trackCount];
  for (int i = 0; i < *trackCount; i++) {
    track_array[i] = reinterpret_cast<lkRtcAudioTrack*>(
        webrtc::make_ref_counted<livekit::AudioTrack>(audio_tracks[i])
            .release());
  }
  return track_array;
}

lkRtcVideoTrack** lkMediaStreamGetVideoTracks(lkMediaStream* stream,
                                              int* trackCount) {
  auto media_stream =
      reinterpret_cast<livekit::MediaStream*>(stream)->media_stream();
  auto video_tracks = media_stream->GetVideoTracks();
  *trackCount = static_cast<int>(video_tracks.size());
  if (*trackCount == 0) {
    return nullptr;
  }
  lkRtcVideoTrack** track_array = new lkRtcVideoTrack*[*trackCount];
  for (int i = 0; i < *trackCount; i++) {
    track_array[i] = reinterpret_cast<lkRtcVideoTrack*>(
        webrtc::make_ref_counted<livekit::VideoTrack>(video_tracks[i])
            .release());
  }
  return track_array;
}

int lkMediaStreamGetIdLength(lkMediaStream* stream) {
  auto id = reinterpret_cast<livekit::MediaStream*>(stream)->id();
  return static_cast<int>(id.size());
}

int lkMediaStreamGetId(lkMediaStream* stream, char* buffer, int bufferSize) {
  auto id = reinterpret_cast<livekit::MediaStream*>(stream)->id();
  int len = static_cast<int>(id.size());
  if (bufferSize > 0) {
    int copySize = (len < bufferSize) ? len : bufferSize;
    memcpy(buffer, id.c_str(), copySize);
  }
  return len;
}

lkNativeVideoSink* lkCreateNativeVideoSink(
    const lkVideoSinkCallabacks* callbacks,
    void* userdata) {
  return reinterpret_cast<lkNativeVideoSink*>(
      webrtc::make_ref_counted<livekit::NativeVideoSink>(callbacks, userdata)
          .release());
}

void lkVideoTrackAddSink(lkRtcVideoTrack* track, lkNativeVideoSink* sink) {
  reinterpret_cast<livekit::VideoTrack*>(track)->add_sink(
      webrtc::scoped_refptr<livekit::NativeVideoSink>(
          reinterpret_cast<livekit::NativeVideoSink*>(sink)));
}

void lkVideoTrackRemoveSink(lkRtcVideoTrack* track, lkNativeVideoSink* sink) {
  reinterpret_cast<livekit::VideoTrack*>(track)->remove_sink(
      webrtc::scoped_refptr<livekit::NativeVideoSink>(
          reinterpret_cast<livekit::NativeVideoSink*>(sink)));
}

lkVideoTrackSource* lkCreateVideoTrackSource(lkVideoResolution resolution) {
  return reinterpret_cast<lkVideoTrackSource*>(
      webrtc::make_ref_counted<livekit::VideoTrackSource>(resolution)
          .release());
}

lkVideoFrameBufferType lkVideoFrameBufferGetType(
    lkVideoFrameBuffer* frameBuffer) {
  return reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)
      ->buffer_type();
}

uint32_t lkVideoFrameBufferGetWidth(lkVideoFrameBuffer* frameBuffer) {
  return reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)->width();
}

uint32_t lkVideoFrameBufferGetHeight(lkVideoFrameBuffer* frameBuffer) {
  return reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)->height();
}

LK_EXPORT lkI420Buffer* lkVideoFrameBufferToI420(
    lkVideoFrameBuffer* frameBuffer) {
  auto i420_buffer =
      reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)->to_i420();
  if (!i420_buffer) {
    return nullptr;
  }
  return reinterpret_cast<lkI420Buffer*>(i420_buffer.release());
}

lkI420Buffer* lkVideoFrameBufferGetI420(lkVideoFrameBuffer* frameBuffer) {
  return reinterpret_cast<lkI420Buffer*>(
      reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)
          ->get_i420()
          .release());
}

lkI420ABuffer* lkVideoFrameBufferGetI420A(lkVideoFrameBuffer* frameBuffer) {
  return reinterpret_cast<lkI420ABuffer*>(
      reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)
          ->get_i420a()
          .release());
}

lkI422Buffer* lkVideoFrameBufferGetI422(lkVideoFrameBuffer* frameBuffer) {
  return reinterpret_cast<lkI422Buffer*>(
      reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)
          ->get_i422()
          .release());
}

lkI444Buffer* lkVideoFrameBufferGetI444(lkVideoFrameBuffer* frameBuffer) {
  return reinterpret_cast<lkI444Buffer*>(
      reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)
          ->get_i444()
          .release());
}

lkI010Buffer* lkVideoFrameBufferGetI010(lkVideoFrameBuffer* frameBuffer) {
  return reinterpret_cast<lkI010Buffer*>(
      reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)
          ->get_i010()
          .release());
}

lkNV12Buffer* lkVideoFrameBufferGetNV12(lkVideoFrameBuffer* frameBuffer) {
  return reinterpret_cast<lkNV12Buffer*>(
      reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)
          ->get_nv12()
          .release());
}

LK_EXPORT lkI420Buffer* lkI420BufferNew(uint32_t width,
                                        uint32_t height,
                                        uint32_t stride_y,
                                        uint32_t stride_u,
                                        uint32_t stride_v) {
  return reinterpret_cast<lkI420Buffer*>(
      livekit::new_i420_buffer(width, height, stride_y, stride_u, stride_v)
          .release());
}

uint32_t lkI420BufferGetChromaWidth(lkI420Buffer* buffer) {
  return reinterpret_cast<livekit::I420Buffer*>(buffer)->chroma_width();
}

uint32_t lkI420BufferGetChromaHeight(lkI420Buffer* buffer) {
  return reinterpret_cast<livekit::I420Buffer*>(buffer)->chroma_height();
}

uint32_t lkI420BufferGetStrideY(lkI420Buffer* buffer) {
  return reinterpret_cast<livekit::I420Buffer*>(buffer)->stride_y();
}

uint32_t lkI420BufferGetStrideU(lkI420Buffer* buffer) {
  return reinterpret_cast<livekit::I420Buffer*>(buffer)->stride_u();
}

uint32_t lkI420BufferGetStrideV(lkI420Buffer* buffer) {
  return reinterpret_cast<livekit::I420Buffer*>(buffer)->stride_v();
}

const uint8_t* lkI420BufferGetDataY(lkI420Buffer* buffer) {
  return reinterpret_cast<livekit::I420Buffer*>(buffer)->data_y();
}

const uint8_t* lkI420BufferGetDataU(lkI420Buffer* buffer) {
  return reinterpret_cast<livekit::I420Buffer*>(buffer)->data_u();
}

const uint8_t* lkI420BufferGetDataV(lkI420Buffer* buffer) {
  return reinterpret_cast<livekit::I420Buffer*>(buffer)->data_v();
}

lkI420Buffer* lkI420BufferScale(lkI420Buffer* buffer,
                                int scaledWidth,
                                int scaledHeight) {
  return reinterpret_cast<lkI420Buffer*>(
      reinterpret_cast<livekit::I420Buffer*>(buffer)
          ->scale(scaledWidth, scaledHeight)
          .release());
}

uint32_t lkI420ABufferGetChromaWidth(lkI420ABuffer* buffer) {
  return reinterpret_cast<livekit::I420ABuffer*>(buffer)->chroma_width();
}

uint32_t lkI420ABufferGetChromaHeight(lkI420ABuffer* buffer) {
  return reinterpret_cast<livekit::I420ABuffer*>(buffer)->chroma_height();
}

uint32_t lkI420ABufferGetStrideY(lkI420ABuffer* buffer) {
  return reinterpret_cast<livekit::I420ABuffer*>(buffer)->stride_y();
}

uint32_t lkI420ABufferGetStrideU(lkI420ABuffer* buffer) {
  return reinterpret_cast<livekit::I420ABuffer*>(buffer)->stride_u();
}

uint32_t lkI420ABufferGetStrideV(lkI420ABuffer* buffer) {
  return reinterpret_cast<livekit::I420ABuffer*>(buffer)->stride_v();
}

uint32_t lkI420ABufferGetStrideA(lkI420ABuffer* buffer) {
  return reinterpret_cast<livekit::I420ABuffer*>(buffer)->stride_a();
}

const uint8_t* lkI420ABufferGetDataA(lkI420ABuffer* buffer) {
  return reinterpret_cast<livekit::I420ABuffer*>(buffer)->data_a();
}

lkI420ABuffer* lkI420ABufferScale(lkI420ABuffer* buffer,
                                  int scaledWidth,
                                  int scaledHeight) {
  return reinterpret_cast<lkI420ABuffer*>(
      reinterpret_cast<livekit::I420ABuffer*>(buffer)
          ->scale(scaledWidth, scaledHeight)
          .release());
}

lkI422Buffer* lkI422BufferNew(uint32_t width,
                              uint32_t height,
                              uint32_t stride_y,
                              uint32_t stride_u,
                              uint32_t stride_v) {
  return reinterpret_cast<lkI422Buffer*>(
      livekit::new_i422_buffer(width, height, stride_y, stride_u, stride_v)
          .release());
}

lkI422Buffer* lkI422BufferScale(lkI422Buffer* buffer,
                                int scaledWidth,
                                int scaledHeight) {
  return reinterpret_cast<lkI422Buffer*>(
      reinterpret_cast<livekit::I422Buffer*>(buffer)
          ->scale(scaledWidth, scaledHeight)
          .release());
}

uint32_t lkI422BufferGetChromaWidth(lkI422Buffer* buffer) {
  return reinterpret_cast<livekit::I422Buffer*>(buffer)->chroma_width();
}

uint32_t lkI422BufferGetChromaHeight(lkI422Buffer* buffer) {
  return reinterpret_cast<livekit::I422Buffer*>(buffer)->chroma_height();
}

uint32_t lkI422BufferGetStrideY(lkI422Buffer* buffer) {
  return reinterpret_cast<livekit::I422Buffer*>(buffer)->stride_y();
}

uint32_t lkI422BufferGetStrideU(lkI422Buffer* buffer) {
  return reinterpret_cast<livekit::I422Buffer*>(buffer)->stride_u();
}

uint32_t lkI422BufferGetStrideV(lkI422Buffer* buffer) {
  return reinterpret_cast<livekit::I422Buffer*>(buffer)->stride_v();
}

const uint8_t* lkI422BufferGetDataY(lkI422Buffer* buffer) {
  return reinterpret_cast<livekit::I422Buffer*>(buffer)->data_y();
}

const uint8_t* lkI422BufferGetDataU(lkI422Buffer* buffer) {
  return reinterpret_cast<livekit::I422Buffer*>(buffer)->data_u();
}

const uint8_t* lkI422BufferGetDataV(lkI422Buffer* buffer) {
  return reinterpret_cast<livekit::I422Buffer*>(buffer)->data_v();
}

lkI444Buffer* lkI444BufferNew(uint32_t width,
                              uint32_t height,
                              uint32_t stride_y,
                              uint32_t stride_u,
                              uint32_t stride_v) {
  return reinterpret_cast<lkI444Buffer*>(
      livekit::new_i444_buffer(width, height, stride_y, stride_u, stride_v)
          .release());
}

uint32_t lkI444BufferGetChromaWidth(lkI444Buffer* buffer) {
  return reinterpret_cast<livekit::I444Buffer*>(buffer)->chroma_width();
}

uint32_t lkI444BufferGetChromaHeight(lkI444Buffer* buffer) {
  return reinterpret_cast<livekit::I444Buffer*>(buffer)->chroma_height();
}

uint32_t lkI444BufferGetStrideY(lkI444Buffer* buffer) {
  return reinterpret_cast<livekit::I444Buffer*>(buffer)->stride_y();
}

uint32_t lkI444BufferGetStrideU(lkI444Buffer* buffer) {
  return reinterpret_cast<livekit::I444Buffer*>(buffer)->stride_u();
}

uint32_t lkI444BufferGetStrideV(lkI444Buffer* buffer) {
  return reinterpret_cast<livekit::I444Buffer*>(buffer)->stride_v();
}

const uint8_t* lkI444BufferGetDataY(lkI444Buffer* buffer) {
  return reinterpret_cast<livekit::I444Buffer*>(buffer)->data_y();
}

const uint8_t* lkI444BufferGetDataU(lkI444Buffer* buffer) {
  return reinterpret_cast<livekit::I444Buffer*>(buffer)->data_u();
}

const uint8_t* lkI444BufferGetDataV(lkI444Buffer* buffer) {
  return reinterpret_cast<livekit::I444Buffer*>(buffer)->data_v();
}

lkI444Buffer* lkI444BufferScale(lkI444Buffer* buffer,
                                int scaledWidth,
                                int scaledHeight) {
  return reinterpret_cast<lkI444Buffer*>(
      reinterpret_cast<livekit::I444Buffer*>(buffer)
          ->scale(scaledWidth, scaledHeight)
          .release());
}

lkI010Buffer* lkI010BufferNew(uint32_t width,
                              uint32_t height,
                              uint32_t stride_y,
                              uint32_t stride_u,
                              uint32_t stride_v) {
  return reinterpret_cast<lkI010Buffer*>(
      livekit::new_i010_buffer(width, height, stride_y, stride_u, stride_v)
          .release());
}

uint32_t lkI010BufferGetChromaWidth(lkI010Buffer* buffer) {
  return reinterpret_cast<livekit::I010Buffer*>(buffer)->chroma_width();
}

uint32_t lkI010BufferGetChromaHeight(lkI010Buffer* buffer) {
  return reinterpret_cast<livekit::I010Buffer*>(buffer)->chroma_height();
}

uint32_t lkI010BufferGetStrideY(lkI010Buffer* buffer) {
  return reinterpret_cast<livekit::I010Buffer*>(buffer)->stride_y();
}

uint32_t lkI010BufferGetStrideU(lkI010Buffer* buffer) {
  return reinterpret_cast<livekit::I010Buffer*>(buffer)->stride_u();
}

uint32_t lkI010BufferGetStrideV(lkI010Buffer* buffer) {
  return reinterpret_cast<livekit::I010Buffer*>(buffer)->stride_v();
}

const uint16_t* lkI010BufferGetDataY(lkI010Buffer* buffer) {
  return reinterpret_cast<livekit::I010Buffer*>(buffer)->data_y();
}

const uint16_t* lkI010BufferGetDataU(lkI010Buffer* buffer) {
  return reinterpret_cast<livekit::I010Buffer*>(buffer)->data_u();
}

const uint16_t* lkI010BufferGetDataV(lkI010Buffer* buffer) {
  return reinterpret_cast<livekit::I010Buffer*>(buffer)->data_v();
}

lkI010Buffer* lkI010BufferScale(lkI010Buffer* buffer,
                                int scaledWidth,
                                int scaledHeight) {
  return reinterpret_cast<lkI010Buffer*>(
      reinterpret_cast<livekit::I010Buffer*>(buffer)
          ->scale(scaledWidth, scaledHeight)
          .release());
}

lkNV12Buffer* lkNV12BufferNew(uint32_t width,
                              uint32_t height,
                              uint32_t stride_y,
                              uint32_t stride_uv) {
  return reinterpret_cast<lkNV12Buffer*>(
      livekit::new_nv12_buffer(width, height, stride_y, stride_uv).release());
}

uint32_t lkNV12BufferGetChromaWidth(lkNV12Buffer* buffer) {
  return reinterpret_cast<livekit::NV12Buffer*>(buffer)->chroma_width();
}

uint32_t lkNV12BufferGetChromaHeight(lkNV12Buffer* buffer) {
  return reinterpret_cast<livekit::NV12Buffer*>(buffer)->chroma_height();
}

uint32_t lkNV12BufferGetStrideY(lkNV12Buffer* buffer) {
  return reinterpret_cast<livekit::NV12Buffer*>(buffer)->stride_y();
}

uint32_t lkNV12BufferGetStrideUV(lkNV12Buffer* buffer) {
  return reinterpret_cast<livekit::NV12Buffer*>(buffer)->stride_uv();
}

const uint8_t* lkNV12BufferGetDataY(lkNV12Buffer* buffer) {
  return reinterpret_cast<livekit::NV12Buffer*>(buffer)->data_y();
}

const uint8_t* lkNV12BufferGetDataUV(lkNV12Buffer* buffer) {
  return reinterpret_cast<livekit::NV12Buffer*>(buffer)->data_uv();
}

lkNV12Buffer* lkNV12BufferScale(lkNV12Buffer* buffer,
                                int scaledWidth,
                                int scaledHeight) {
  return reinterpret_cast<lkNV12Buffer*>(
      reinterpret_cast<livekit::NV12Buffer*>(buffer)
          ->scale(scaledWidth, scaledHeight)
          .release());
}

void lkVideoFrameBufferToARGB(lkVideoFrameBuffer* frameBuffer,
                              lkVideoFrameBufferType type,
                              uint8_t* argbBuffer,
                              uint32_t stride,
                              uint32_t width,
                              uint32_t height) {}

lkVideoFrameBuffer* lkNewNativeBufferFromPlatformImageBuffer(
    lkPlatformImageBuffer* buffer) {
  auto ptr = livekit::new_native_buffer_from_platform_image_buffer(
#if defined(__APPLE__)
      reinterpret_cast<livekit::PlatformImageBuffer*>(buffer)
#else
      buffer
#endif
  );
  if (!ptr) {
    return nullptr;
  }
  return reinterpret_cast<lkVideoFrameBuffer*>(ptr.release());
}

lkPlatformImageBuffer* lkNativeBufferToPlatformImageBuffer(
    lkVideoFrameBuffer* frameBuffer) {
  return livekit::native_buffer_to_platform_image_buffer(
      webrtc::scoped_refptr<livekit::VideoFrameBuffer>(
          reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)));
}