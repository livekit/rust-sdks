#include "livekit_rtc/include/capi.h"

#include "api/make_ref_counted.h"
#include "livekit_rtc/audio_track.h"
#include "livekit_rtc/data_channel.h"
#include "livekit_rtc/ice_candidate.h"
#include "livekit_rtc/media_stream.h"
#include "livekit_rtc/media_stream_track.h"
#include "livekit_rtc/peer.h"
#include "livekit_rtc/rtp_sender.h"
#include "livekit_rtc/rtp_transceiver.h"
#include "livekit_rtc/session_description.h"
#include "livekit_rtc/utils.h"
#include "livekit_rtc/video_frame.h"
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

lkString* lkCreateString(const char* str) {
  return reinterpret_cast<lkString*>(
      webrtc::make_ref_counted<livekit::LKString>(str).release());
}

int lkStringGetLength(lkString* str) {
  return reinterpret_cast<livekit::LKString*>(str)->length();
}

int lkStringGetData(lkString* str, char* buffer, int bufferSize) {
  auto s = reinterpret_cast<livekit::LKString*>(str);
  int len = static_cast<int>(s->length());
  if (bufferSize > 0) {
    int copySize = (len < bufferSize) ? len : bufferSize;
    memcpy(buffer, s->data(), copySize);
  }
  return len;
}

lkData* lkCreateData(const uint8_t* data, uint32_t size) {
  std::vector<uint8_t> vec(data, data + size);
  return reinterpret_cast<lkData*>(
      webrtc::make_ref_counted<livekit::LKData>(vec).release());
}

int lkDataGetSize(lkData* data) {
  return reinterpret_cast<livekit::LKData*>(data)->size();
}

const uint8_t* lkDataGetData(lkData* data) {
  return reinterpret_cast<livekit::LKData*>(data)->data();
}

lkVectorGeneric* lkCreateVectorGeneric() {
  return reinterpret_cast<lkVectorGeneric*>(
      webrtc::make_ref_counted<livekit::LKVector<lkRefCountedObject*>>()
          .release());
}

uint32_t lkVectorGenericGetSize(lkVectorGeneric* vec) {
  return reinterpret_cast<livekit::LKVector<lkRefCountedObject*>*>(vec)->size();
}

lkRefCountedObject* lkVectorGenericGetAt(lkVectorGeneric* vec, uint32_t index) {
  return reinterpret_cast<livekit::LKVector<lkRefCountedObject*>*>(vec)->get_at(
      index);
}

uint32_t lkVectorGenericPushBack(lkVectorGeneric* vec,
                                 lkRefCountedObject* value) {
  auto lkVec = reinterpret_cast<livekit::LKVector<lkRefCountedObject*>*>(vec);
  lkVec->push_back(value);
  return static_cast<uint32_t>(lkVec->size());
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

lkRtpSender* lkPeerAddTrack(lkPeer* peer,
                            lkMediaStreamTrack* track,
                            const char** streamIds,
                            int streamIdCount,
                            lkRtcError* error) {
  return reinterpret_cast<livekit::Peer*>(peer)->AddTrack(track, streamIds,
                                                          streamIdCount, error);
}

bool lkPeerRemoveTrack(lkPeer* peer, lkRtpSender* sender, lkRtcError* error) {
  return reinterpret_cast<livekit::Peer*>(peer)->RemoveTrack(
      reinterpret_cast<livekit::RtpSender*>(sender), error);
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

lkVectorGeneric* lkPeerGetTransceivers(lkPeer* peer) {
  return reinterpret_cast<livekit::Peer*>(peer)->GetTransceivers();
}

lkVectorGeneric* lkPeerGetSenders(lkPeer* peer) {
  return reinterpret_cast<livekit::Peer*>(peer)->GetSenders();
}

lkVectorGeneric* lkPeerGetReceivers(lkPeer* peer) {
  return reinterpret_cast<livekit::Peer*>(peer)->GetReceivers();
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

lkString* lkDcGetLabel(lkDataChannel* dc) {
  auto label = reinterpret_cast<livekit::DataChannel*>(dc)->label();
  return reinterpret_cast<lkString*>(
      livekit::LKString::Create(label).release());
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

lkString* lkSessionDescriptionGetSdp(lkSessionDescription* desc) {
  std::string sdp =
      reinterpret_cast<livekit::SessionDescription*>(desc)->ToString();
  return reinterpret_cast<lkString*>(livekit::LKString::Create(sdp).release());
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

lkString* lkIceCandidateGetMid(lkIceCandidate* candidate) {
  auto mid = reinterpret_cast<livekit::IceCandidate*>(candidate)->mid();
  return reinterpret_cast<lkString*>(livekit::LKString::Create(mid).release());
}

lkString* lkIceCandidateGetSdp(lkIceCandidate* candidate) {
  std::string sdp = reinterpret_cast<livekit::IceCandidate*>(candidate)->sdp();
  return reinterpret_cast<lkString*>(livekit::LKString::Create(sdp).release());
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

lkRtpCapabilities* lkGetRtpSenderCapabilities(lkPeerFactory* factory,
                                              lkMediaType type) {
  auto peer_factory = reinterpret_cast<livekit::PeerFactory*>(factory);
  return peer_factory->GetRtpSenderCapabilities(type);
}

lkRtpCapabilities* lkGetRtpReceiverCapabilities(lkPeerFactory* factory,
                                                lkMediaType type) {
  auto peer_factory = reinterpret_cast<livekit::PeerFactory*>(factory);
  return peer_factory->GetRtpReceiverCapabilities(type);
}

lkVectorGeneric* lkRtpCapabilitiesGetCodecs(lkRtpCapabilities* capabilities) {
  return reinterpret_cast<livekit::RtpCapabilities*>(capabilities)->GetCodecs();
}

lkVectorGeneric* lkRtpCapabilitiesGetHeaderExtensions(
    lkRtpCapabilities* capabilities) {
  return reinterpret_cast<livekit::RtpCapabilities*>(capabilities)
      ->GetHeaderExtensions();
}

lkRtcVideoTrack* CreateVideoTrack(lkPeerFactory* factory,
                                  const char* id,
                                  lkVideoTrackSource* source) {
  auto peer_factory = reinterpret_cast<livekit::PeerFactory*>(factory);
  return peer_factory->CreateVideoTrack(id, source);
}

lkRtcAudioTrack* CreateAudioTrack(lkPeerFactory* factory,
                                  const char* id,
                                  lkAudioTrackSource* source) {
  auto peer_factory = reinterpret_cast<livekit::PeerFactory*>(factory);
  return peer_factory->CreateAudioTrack(id, source);
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

lkString* lkMediaStreamTrackGetId(lkMediaStreamTrack* track) {
  auto id = reinterpret_cast<livekit::MediaStreamTrack*>(track)->id();
  return reinterpret_cast<lkString*>(livekit::LKString::Create(id).release());
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

lkVectorGeneric* lkMediaStreamGetAudioTracks(lkMediaStream* stream) {
  auto media_stream =
      reinterpret_cast<livekit::MediaStream*>(stream)->media_stream();
  auto audio_tracks = media_stream->GetAudioTracks();
  int trackCount = static_cast<int>(audio_tracks.size());
  if (trackCount == 0) {
    return nullptr;
  }
  auto track_array = webrtc::make_ref_counted<
      livekit::LKVector<webrtc::scoped_refptr<livekit::AudioTrack>>>();

  for (int i = 0; i < trackCount; i++) {
    track_array->push_back(
        webrtc::make_ref_counted<livekit::AudioTrack>(audio_tracks[i]));
  }
  return reinterpret_cast<lkVectorGeneric*>(track_array.release());
}

lkVectorGeneric* lkMediaStreamGetVideoTracks(lkMediaStream* stream) {
  auto media_stream =
      reinterpret_cast<livekit::MediaStream*>(stream)->media_stream();
  auto video_tracks = media_stream->GetVideoTracks();
  int trackCount = static_cast<int>(video_tracks.size());
  if (trackCount == 0) {
    return nullptr;
  }
  auto track_array = webrtc::make_ref_counted<
      livekit::LKVector<webrtc::scoped_refptr<livekit::VideoTrack>>>();

  for (int i = 0; i < trackCount; i++) {
    track_array->push_back(
        webrtc::make_ref_counted<livekit::VideoTrack>(video_tracks[i]));
  }
  return reinterpret_cast<lkVectorGeneric*>(track_array.release());
}

lkString* lkMediaStreamGetId(lkMediaStream* stream) {
  auto id = reinterpret_cast<livekit::MediaStream*>(stream)->id();
  return reinterpret_cast<lkString*>(livekit::LKString::Create(id).release()); 
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

lkVideoResolution lkVideoTrackSourceGetResolution(lkVideoTrackSource* source) {
  return reinterpret_cast<livekit::VideoTrackSource*>(source)
      ->video_resolution();
}

lkVideoBufferType lkVideoFrameBufferGetType(lkVideoFrameBuffer* frameBuffer) {
  return reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)
      ->buffer_type();
}

uint32_t lkVideoFrameBufferGetWidth(lkVideoFrameBuffer* frameBuffer) {
  return reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)->width();
}

uint32_t lkVideoFrameBufferGetHeight(lkVideoFrameBuffer* frameBuffer) {
  return reinterpret_cast<livekit::VideoFrameBuffer*>(frameBuffer)->height();
}

lkI420Buffer* lkVideoFrameBufferToI420(lkVideoFrameBuffer* frameBuffer) {
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

lkI420Buffer* lkI420BufferNew(uint32_t width,
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
                              lkVideoBufferType type,
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

lkVideoFrameBuilder* lkCreateVideoFrameBuilder() {
  return reinterpret_cast<lkVideoFrameBuilder*>(
      webrtc::make_ref_counted<livekit::VideoFrameBuilder>().release());
}

void lkVideoFrameBuilderSetVideoFrameBuffer(lkVideoFrameBuilder* builder,
                                            lkVideoFrameBuffer* buffer) {
  reinterpret_cast<livekit::VideoFrameBuilder*>(builder)
      ->set_video_frame_buffer(
          *webrtc::scoped_refptr<livekit::VideoFrameBuffer>(
               reinterpret_cast<livekit::VideoFrameBuffer*>(buffer))
               .get());
}

void lkVideoFrameBuilderSetTimestampUs(lkVideoFrameBuilder* builder,
                                       int64_t timestampNs) {
  reinterpret_cast<livekit::VideoFrameBuilder*>(builder)->set_timestamp_us(
      timestampNs);
}

void lkVideoFrameBuilderSetRotation(lkVideoFrameBuilder* builder,
                                    lkVideoRotation rotation) {
  reinterpret_cast<livekit::VideoFrameBuilder*>(builder)->set_rotation(
      rotation);
}

void lkVideoFrameBuilderSetId(lkVideoFrameBuilder* builder, uint16_t id) {
  reinterpret_cast<livekit::VideoFrameBuilder*>(builder)->set_id(id);
}

lkVideoFrame* lkVideoFrameBuilderBuild(lkVideoFrameBuilder* builder) {
  auto frame = reinterpret_cast<livekit::VideoFrameBuilder*>(builder)->build();
  if (!frame) {
    return nullptr;
  }
  return reinterpret_cast<lkVideoFrame*>(frame.release());
}

void lkVideoTrackSourceOnCaptureFrame(lkVideoTrackSource* source,
                                      lkVideoFrame* frame) {
  auto video_frame = webrtc::scoped_refptr<livekit::VideoFrame>(
      reinterpret_cast<livekit::VideoFrame*>(frame));
  reinterpret_cast<livekit::VideoTrackSource*>(source)->on_captured_frame(
      video_frame);
}

lkVideoRotation lkVideoFrameGetRotation(const lkVideoFrame* frame) {
  return static_cast<lkVideoRotation>(
      reinterpret_cast<const livekit::VideoFrame*>(frame)->rotation());
}

int64_t lkVideoFrameGetTimestampUs(const lkVideoFrame* frame) {
  return reinterpret_cast<const livekit::VideoFrame*>(frame)->timestamp_us();
}

uint16_t lkVideoFrameGetId(const lkVideoFrame* frame) {
  return reinterpret_cast<const livekit::VideoFrame*>(frame)->id();
}

lkVideoFrameBuffer* lkVideoFrameGetBuffer(const lkVideoFrame* frame) {
  return reinterpret_cast<lkVideoFrameBuffer*>(
      reinterpret_cast<const livekit::VideoFrame*>(frame)
          ->video_frame_buffer()
          .release());
}

lkMediaStreamTrack* lkRtpSenderGetTrack(lkRtpSender* sender) {
  return reinterpret_cast<lkMediaStreamTrack*>(
      reinterpret_cast<livekit::RtpSender*>(sender)->track().release());
}

bool lkRtpSenderSetTrack(lkRtpSender* sender, lkMediaStreamTrack* track) {
  return reinterpret_cast<livekit::RtpSender*>(sender)->set_track(
      webrtc::scoped_refptr<livekit::MediaStreamTrack>(
          reinterpret_cast<livekit::MediaStreamTrack*>(track)));
}

lkString* lkRtpTransceiverGetMid(lkRtpTransceiver* transceiver) {
  auto mid = reinterpret_cast<livekit::RtpTransceiver*>(transceiver)->mid();
  return reinterpret_cast<lkString*>(livekit::LKString::Create(mid).release());
}

lkRtpTransceiverDirection lkRtpTransceiverGetDirection(
    lkRtpTransceiver* transceiver) {
  return static_cast<lkRtpTransceiverDirection>(
      reinterpret_cast<livekit::RtpTransceiver*>(transceiver)->direction());
}

lkRtpTransceiverDirection lkRtpTransceiverCurrentDirection(
    lkRtpTransceiver* transceiver) {
  return reinterpret_cast<livekit::RtpTransceiver*>(transceiver)
      ->current_direction();
}

lkRtpSender* lkRtpTransceiverGetSender(lkRtpTransceiver* transceiver) {
  return reinterpret_cast<lkRtpSender*>(
      reinterpret_cast<livekit::RtpTransceiver*>(transceiver)
          ->sender()
          .release());
}

lkRtpReceiver* lkRtpTransceiverGetReceiver(lkRtpTransceiver* transceiver) {
  return reinterpret_cast<lkRtpReceiver*>(
      reinterpret_cast<livekit::RtpTransceiver*>(transceiver)
          ->receiver()
          .release());
}

void lkRtpTransceiverStop(lkRtpTransceiver* transceiver) {
  reinterpret_cast<livekit::RtpTransceiver*>(transceiver)->stop_standard();
}

lkMediaStreamTrack* lkRtpReceiverGetTrack(lkRtpReceiver* receiver) {
  return reinterpret_cast<lkMediaStreamTrack*>(
      reinterpret_cast<livekit::RtpReceiver*>(receiver)->track().release());
}

void lkPeerGetStats(lkPeer* peer,
                    void (*onComplete)(const char* statsJson, void* userdata),
                    void* userdata) {
  // TODO: implement
}

void lkRtpSenderGetStats(lkRtpSender* sender,
                         void (*onComplete)(const char* statsJson,
                                            void* userdata),
                         void* userdata) {
  reinterpret_cast<livekit::RtpSender*>(sender)->get_stats(onComplete,
                                                           userdata);
}

void lkRtpReceiverGetStats(lkRtpReceiver* receiver,
                           void (*onComplete)(const char* statsJson,
                                              void* userdata),
                           void* userdata) {
  reinterpret_cast<livekit::RtpReceiver*>(receiver)->get_stats(onComplete,
                                                               userdata);
}

uint16_t lkRtpCodecCapabilityGetChannels(lkRtpCodecCapability* codec) {
  return reinterpret_cast<livekit::RtpCodecCapability*>(codec)->num_channels();
}

uint32_t lkRtpCodecCapabilityGetClockRate(lkRtpCodecCapability* codec) {
  return reinterpret_cast<livekit::RtpCodecCapability*>(codec)->clock_rate();
}

lkString* lkRtpCodecCapabilityGetMimeType(lkRtpCodecCapability* codec) {
  auto mime_type =
      reinterpret_cast<livekit::RtpCodecCapability*>(codec)->mime_type();
  return reinterpret_cast<lkString*>(
      livekit::LKString::Create(mime_type).release());
}

lkString* lkRtpCodecCapabilityGetSdpFmtpLine(lkRtpCodecCapability* codec) {
  auto sdp_fmtp_line =
      reinterpret_cast<livekit::RtpCodecCapability*>(codec)->sdp_fmtp_line();
  return reinterpret_cast<lkString*>(
      livekit::LKString::Create(sdp_fmtp_line).release());
}

lkString* lkRtpHeaderExtensionCapabilityGetUri(
    lkRtpHeaderExtensionCapability* ext) {
  auto uri =
      reinterpret_cast<livekit::RtpHeaderExtensionCapability*>(ext)->uri();
  return reinterpret_cast<lkString*>(livekit::LKString::Create(uri).release());
}

lkRtpTransceiverDirection lkRtpHeaderExtensionCapabilityGetDirection(
    lkRtpHeaderExtensionCapability* ext) {
  return static_cast<lkRtpTransceiverDirection>(
      reinterpret_cast<livekit::RtpHeaderExtensionCapability*>(ext)
          ->direction());
}

lkString* lkRtcpParametersGetCname(lkRtcpParameters* rtcp) {
  auto cname = reinterpret_cast<livekit::RtcpParameters*>(rtcp)->cname();
  return reinterpret_cast<lkString*>(
      livekit::LKString::Create(cname).release());
}

bool lkRtcpParametersGetReducedSize(lkRtcpParameters* rtcp) {
  return reinterpret_cast<livekit::RtcpParameters*>(rtcp)->reduced_size();
}

uint8_t lkRtpCodecParametersGetPayloadType(lkRtpCodecParameters* codec) {
  return reinterpret_cast<livekit::RtpCodecParameters*>(codec)->payload_type();
}

lkString* lkRtpCodecParametersGetMimeType(lkRtpCodecParameters* codec) {
  auto mime_type =
      reinterpret_cast<livekit::RtpCodecParameters*>(codec)->mime_type();
  return reinterpret_cast<lkString*>(
      livekit::LKString::Create(mime_type).release());
}

uint32_t lkRtpCodecParametersGetClockRate(lkRtpCodecParameters* codec) {
  return reinterpret_cast<livekit::RtpCodecParameters*>(codec)->clock_rate();
}

uint16_t lkRtpCodecParametersGetChannels(lkRtpCodecParameters* codec) {
  return reinterpret_cast<livekit::RtpCodecParameters*>(codec)->num_channels();
}

lkString* lkRtpHeaderExtensionParametersGetUri(
    lkRtpHeaderExtensionParameters* ext) {
  auto uri =
      reinterpret_cast<livekit::RtpHeaderExtensionParameters*>(ext)->uri();
  return reinterpret_cast<lkString*>(livekit::LKString::Create(uri).release());
}

uint8_t lkRtpHeaderExtensionParametersGetId(
    lkRtpHeaderExtensionParameters* ext) {
  return reinterpret_cast<livekit::RtpHeaderExtensionParameters*>(ext)->id();
}

bool lkRtpHeaderExtensionParametersGetEncrypted(
    lkRtpHeaderExtensionParameters* ext) {
  return reinterpret_cast<livekit::RtpHeaderExtensionParameters*>(ext)
      ->encrypted();
}

lkVectorGeneric* lkRtpParametersGetCodecs(lkRtpParameters* params) {
  return reinterpret_cast<livekit::RtpParameters*>(params)->GetCodecs();
}

lkRtcpParameters* lkRtpParametersGetRtcp(lkRtpParameters* params) {
  webrtc::scoped_refptr<livekit::RtcpParameters> rtcp =
      reinterpret_cast<livekit::RtpParameters*>(params)->rtcp;
  return reinterpret_cast<lkRtcpParameters*>(rtcp.release());
}

lkVectorGeneric* lkRtpParametersGetHeaderExtensions(lkRtpParameters* params) {
  return reinterpret_cast<livekit::RtpParameters*>(params)
      ->GetHeaderExtensions();
}

lkRtpParameters* lkRtpSenderGetParameters(lkRtpSender* sender) {
  return reinterpret_cast<lkRtpParameters*>(
      reinterpret_cast<livekit::RtpSender*>(sender)
          ->get_parameters()
          .release());
}

bool lkRtpSenderSetParameters(lkRtpSender* sender,
                              lkRtpParameters* params,
                              lkRtcError* error) {
  auto p = webrtc::scoped_refptr<livekit::RtpParameters>(
      reinterpret_cast<livekit::RtpParameters*>(params));
  return reinterpret_cast<livekit::RtpSender*>(sender)->set_parameters(p,
                                                                       error);
}

lkRtpParameters* lkRtpReceiverGetParameters(lkRtpReceiver* receiver) {
  return reinterpret_cast<lkRtpParameters*>(
      reinterpret_cast<livekit::RtpReceiver*>(receiver)
          ->get_parameters()
          .release());
}

lkRtpTransceiverInit* lkRtpTransceiverInitCreate() {
  return reinterpret_cast<lkRtpTransceiverInit*>(
      livekit::RtpTransceiverInit::Create().release());
}

void lkRtpTransceiverInitSetDirection(lkRtpTransceiverInit* init,
                                      lkRtpTransceiverDirection direction) {
  reinterpret_cast<livekit::RtpTransceiverInit*>(init)->set_direction(
      direction);
}

void lkRtpTransceiverInitSetStreamIds(lkRtpTransceiverInit* init,
                                      lkVectorGeneric* streamIds) {
  reinterpret_cast<livekit::RtpTransceiverInit*>(init)->set_lk_stream_ids(
      streamIds);
}

lkRtpTransceiverDirection lkRtpTransceiverInitGetDirection(
    lkRtpTransceiverInit* init) {
  return reinterpret_cast<livekit::RtpTransceiverInit*>(init)->direction();
}

void lkRtpTransceiverInitSetSendEncodingsdings(lkRtpTransceiverInit* init,
                                               lkVectorGeneric* encodings) {
  reinterpret_cast<livekit::RtpTransceiverInit*>(init)->set_lk_send_encodings(
      encodings);
}

bool lkRtpTransceiverSetCodecPreferences(lkRtpTransceiver* transceiver,
                                         lkVectorGeneric* codecs,
                                         lkRtcError* error) {
  return reinterpret_cast<livekit::RtpTransceiver*>(transceiver)
      ->lk_set_codec_preferences(codecs, error);
}

bool lkRtpTransceiverStopWithError(lkRtpTransceiver* transceiver,
                                   lkRtcError* error) {
  return reinterpret_cast<livekit::RtpTransceiver*>(transceiver)
      ->stop_with_error(error);
}

lkRtpCodecCapability* lkRtpCodecCapabilityCreate() {
  return reinterpret_cast<lkRtpCodecCapability*>(
      livekit::RtpCodecCapability::Create().release());
}

void lkRtpCodecCapabilitySetMimeType(lkRtpCodecCapability* codec,
                                     const char* mimeType) {
  reinterpret_cast<livekit::RtpCodecCapability*>(codec)->set_mime_type(
      mimeType);
}

void lkRtpCodecCapabilitySetClockRate(lkRtpCodecCapability* codec,
                                      uint32_t clockRate) {
  reinterpret_cast<livekit::RtpCodecCapability*>(codec)->set_clock_rate(
      clockRate);
}

void lkRtpCodecCapabilitySetChannels(lkRtpCodecCapability* codec,
                                     uint16_t channels) {
  reinterpret_cast<livekit::RtpCodecCapability*>(codec)->set_num_channels(
      channels);
}

void lkRtpCodecCapabilitySetSdpFmtpLine(lkRtpCodecCapability* codec,
                                        const char* sdpFmtpLine) {
  reinterpret_cast<livekit::RtpCodecCapability*>(codec)->set_sdp_fmtp_line(
      sdpFmtpLine);
}

lkRtpEncodingParameters* lkRtpEncodingParametersCreate() {
  return reinterpret_cast<lkRtpEncodingParameters*>(
      livekit::RtpEncodingParameters::Create().release());
}

void lkRtpEncodingParametersSetActive(lkRtpEncodingParameters* encoding,
                                      bool active) {
  reinterpret_cast<livekit::RtpEncodingParameters*>(encoding)->set_active(
      active);
}

void lkRtpEncodingParametersSetMaxBitrateBps(lkRtpEncodingParameters* encoding,
                                             int64_t maxBitrateBps) {
  reinterpret_cast<livekit::RtpEncodingParameters*>(encoding)
      ->set_max_bitrate_bps(maxBitrateBps);
}

void lkRtpEncodingParametersSetMinBitrateBps(lkRtpEncodingParameters* encoding,
                                             int64_t minBitrateBps) {
  reinterpret_cast<livekit::RtpEncodingParameters*>(encoding)
      ->set_min_bitrate_bps(minBitrateBps);
}

void lkRtpEncodingParametersSetMaxFramerate(lkRtpEncodingParameters* encoding,
                                            double maxFramerate) {
  reinterpret_cast<livekit::RtpEncodingParameters*>(encoding)
      ->set_max_framerate(maxFramerate);
}

void lkRtpEncodingParametersSetScaleResolutionDownBy(
    lkRtpEncodingParameters* encoding,
    double scaleResolutionDownBy) {
  reinterpret_cast<livekit::RtpEncodingParameters*>(encoding)
      ->set_scale_resolution_down_by(scaleResolutionDownBy);
}

void lkRtpEncodingParametersSetScalabilityMode(
    lkRtpEncodingParameters* encoding,
    const char* scalabilityMode) {
  reinterpret_cast<livekit::RtpEncodingParameters*>(encoding)
      ->set_scalability_mode(scalabilityMode);
}

void lkRtpEncodingParametersSetRid(lkRtpEncodingParameters* encoding,
                                   const char* rid) {
  reinterpret_cast<livekit::RtpEncodingParameters*>(encoding)->set_rid(rid);
}

lkRtpTransceiver* lkPeerAddTransceiver(lkPeer* peer,
                                       lkMediaStreamTrack* track,
                                       lkRtpTransceiverInit* init,
                                       lkRtcError* error) {
  return reinterpret_cast<livekit::Peer*>(peer)->AddTransceiver(track, init,
                                                                error);
}

lkRtpTransceiver* lkPeerAddTransceiverForMedia(lkPeer* peer,
                                               lkMediaType type,
                                               lkRtpTransceiverInit* init,
                                               lkRtcError* error) {
  return reinterpret_cast<livekit::Peer*>(peer)->AddTransceiverForMedia(
      type, init, error);
}

lkRtpParameters* lkRtpParametersCreate() {
  return reinterpret_cast<lkRtpParameters*>(
      livekit::RtpParameters::Create().release());
}

void lkRtpParametersSetCodecs(lkRtpParameters* params,
                              lkVectorGeneric* codecs) {
  reinterpret_cast<livekit::RtpParameters*>(params)->set_lk_codecs(codecs);
}

void lkRtpParametersSetRtcp(lkRtpParameters* params, lkRtcpParameters* rtcp) {
  reinterpret_cast<livekit::RtpParameters*>(params)->set_rtcp(
      webrtc::scoped_refptr<livekit::RtcpParameters>(
          reinterpret_cast<livekit::RtcpParameters*>(rtcp)));
}

void lkRtcpParametersSetReducedSize(lkRtcpParameters* rtcp, bool reducedSize) {
  reinterpret_cast<livekit::RtcpParameters*>(rtcp)->set_reduced_size(
      reducedSize);
}

void lkRtcpParametersSetCname(lkRtcpParameters* rtcp, const char* cname) {
  reinterpret_cast<livekit::RtcpParameters*>(rtcp)->set_cname(cname);
}

void lkRtpParametersSetHeaderExtensions(lkRtpParameters* params,
                                        lkVectorGeneric* headerExtensions) {
  reinterpret_cast<livekit::RtpParameters*>(params)->set_lk_header_extensions(
      headerExtensions);
}

lkRtpCodecParameters* lkRtpCodecParametersCreate() {
  return reinterpret_cast<lkRtpCodecParameters*>(
      livekit::RtpCodecParameters::Create().release());
}

lkRtcpParameters* lkRtcpParametersCreate() {
  return reinterpret_cast<lkRtcpParameters*>(
      livekit::RtcpParameters::Create().release());
}

void lkRtpCodecParametersSetPayloadType(lkRtpCodecParameters* codec,
                                        uint32_t payloadType) {
  reinterpret_cast<livekit::RtpCodecParameters*>(codec)->set_payload_type(
      static_cast<uint8_t>(payloadType));
}

void lkRtpCodecParametersSetMimeType(lkRtpCodecParameters* codec,
                                     const char* mimeType) {
  reinterpret_cast<livekit::RtpCodecParameters*>(codec)->set_mime_type(
      mimeType);
}

void lkRtpCodecParametersSetClockRate(lkRtpCodecParameters* codec,
                                      uint32_t clockRate) {
  reinterpret_cast<livekit::RtpCodecParameters*>(codec)->set_clock_rate(
      clockRate);
}

void lkRtpCodecParametersSetChannels(lkRtpCodecParameters* codec,
                                     uint32_t channels) {
  reinterpret_cast<livekit::RtpCodecParameters*>(codec)->set_num_channels(
      static_cast<uint16_t>(channels));
}

lkRtpHeaderExtensionParameters* lkRtpHeaderExtensionParametersCreate() {
  return reinterpret_cast<lkRtpHeaderExtensionParameters*>(
      livekit::RtpHeaderExtensionParameters::Create().release());
}

void lkRtpHeaderExtensionParametersSetUri(lkRtpHeaderExtensionParameters* ext,
                                          const char* uri) {
  reinterpret_cast<livekit::RtpHeaderExtensionParameters*>(ext)->set_uri(uri);
}

void lkRtpHeaderExtensionParametersSetId(lkRtpHeaderExtensionParameters* ext,
                                         uint32_t id) {
  reinterpret_cast<livekit::RtpHeaderExtensionParameters*>(ext)->set_id(
      static_cast<uint8_t>(id));
}

void lkRtpHeaderExtensionParametersSetEncrypted(
    lkRtpHeaderExtensionParameters* ext,
    bool encrypted) {
  reinterpret_cast<livekit::RtpHeaderExtensionParameters*>(ext)->set_encrypted(
      encrypted);
}
