#pragma once

#include "rust/cxx.h"

namespace livekit {
class MediaStream;
class AudioTrack;
class VideoTrack;
class Candidate;
class RtpSender;
class RtpReceiver;
class RtpTransceiver;
}  // namespace livekit
#include "webrtc-sys/src/helper.rs.h"

namespace livekit {

// Impl not needed
static rust::Vec<MediaStreamPtr> _vec_media_stream_ptr() {
  throw;
}
static rust::Vec<CandidatePtr> _vec_candidate_ptr() {
  throw;
}
static rust::Vec<AudioTrackPtr> _vec_audio_track_ptr() {
  throw;
}
static rust::Vec<VideoTrackPtr> _vec_video_track_ptr() {
  throw;
}
static rust::Vec<RtpSenderPtr> _vec_rtp_sender_ptr() {
  throw;
}
static rust::Vec<RtpReceiverPtr> _vec_rtp_receiver_ptr() {
  throw;
}
static rust::Vec<RtpTransceiverPtr> _vec_rtp_transceiver_ptr() {
  throw;
}

}  // namespace livekit
