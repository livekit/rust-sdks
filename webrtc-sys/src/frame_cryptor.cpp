/*
 * Copyright 2023 LiveKit
 *
 * Licensed under the Apache License, Version 2.0 (the “License”);
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an “AS IS” BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "livekit/frame_cryptor.h"

#include <memory>

#include "absl/types/optional.h"

namespace livekit {

rtc::scoped_refptr<RTCFrameCryptor> FrameCryptorFactory::frameCryptorFromRtpSender(
    const std::string participant_id,
    rtc::scoped_refptr<webrtc::RtpSenderInterface> sender,
    Algorithm algorithm,
    rtc::scoped_refptr<webrtc::KeyProvider> key_provider) {
  return rtc::make_ref_counted<RTCFrameCryptor>(participant_id, algorithm,
                                                   key_provider, sender);
}

/// Create a frame cyrptor from a [RTCRtpReceiver].
rtc::scoped_refptr<RTCFrameCryptor> FrameCryptorFactory::frameCryptorFromRtpReceiver(
    const std::string participant_id,
    rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver,
    Algorithm algorithm,
    rtc::scoped_refptr<webrtc::KeyProvider> key_provider) {
  return rtc::make_ref_counted<RTCFrameCryptor>(participant_id, algorithm,
                                                   key_provider, receiver);
}

webrtc::FrameCryptorTransformer::Algorithm AlgorithmToFrameCryptorAlgorithm(
    Algorithm algorithm) {
  switch (algorithm) {
    case Algorithm::kAesGcm:
      return webrtc::FrameCryptorTransformer::Algorithm::kAesGcm;
    case Algorithm::kAesCbc:
      return webrtc::FrameCryptorTransformer::Algorithm::kAesCbc;
    default:
      return webrtc::FrameCryptorTransformer::Algorithm::kAesGcm;
  }
}

RTCFrameCryptor::RTCFrameCryptor(
    const std::string participant_id,
    Algorithm algorithm,
    rtc::scoped_refptr<webrtc::KeyProvider> key_provider,
    rtc::scoped_refptr<webrtc::RtpSenderInterface> sender)
    : participant_id_(participant_id),
      enabled_(false),
      key_index_(0),
      key_provider_(key_provider),
      sender_(sender) {
  auto mediaType = sender->track()->kind() == "audio"
          ? webrtc::FrameCryptorTransformer::MediaType::kAudioFrame
          : webrtc::FrameCryptorTransformer::MediaType::kVideoFrame;
  e2ee_transformer_ = rtc::scoped_refptr<webrtc::FrameCryptorTransformer>(
      new webrtc::FrameCryptorTransformer(
          participant_id_, mediaType,
          AlgorithmToFrameCryptorAlgorithm(algorithm),
          key_provider_));
  e2ee_transformer_->SetFrameCryptorTransformerObserver(this);
  sender->SetEncoderToPacketizerFrameTransformer(
      e2ee_transformer_);
  e2ee_transformer_->SetEnabled(false);
}

RTCFrameCryptor::RTCFrameCryptor(
    const std::string participant_id,
    Algorithm algorithm,
    rtc::scoped_refptr<webrtc::KeyProvider> key_provider,
    rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver)
    : participant_id_(participant_id),
      enabled_(false),
      key_index_(0),
      key_provider_(key_provider),
      receiver_(receiver) {
  auto mediaType = receiver->track()->kind() == "audio"
          ? webrtc::FrameCryptorTransformer::MediaType::kAudioFrame
          : webrtc::FrameCryptorTransformer::MediaType::kVideoFrame;
  e2ee_transformer_ = rtc::scoped_refptr<webrtc::FrameCryptorTransformer>(
      new webrtc::FrameCryptorTransformer(
          participant_id_, mediaType,
          AlgorithmToFrameCryptorAlgorithm(algorithm),
          key_provider_));
  e2ee_transformer_->SetFrameCryptorTransformerObserver(this);
  receiver->SetDepacketizerToDecoderFrameTransformer(
      e2ee_transformer_);
  e2ee_transformer_->SetEnabled(false);
}

RTCFrameCryptor::~RTCFrameCryptor() {}

bool RTCFrameCryptor::SetEnabled(bool enabled) {
  webrtc::MutexLock lock(&mutex_);
  enabled_ = enabled;
  e2ee_transformer_->SetEnabled(enabled_);
  return true;
}

void RTCFrameCryptor::RegisterRTCFrameCryptorObserver(
    RTCFrameCryptorObserver* observer) {
  webrtc::MutexLock lock(&mutex_);
  observer_ = observer;
}

void RTCFrameCryptor::DeRegisterRTCFrameCryptorObserver() {
  webrtc::MutexLock lock(&mutex_);
  observer_ = nullptr;
  e2ee_transformer_->SetFrameCryptorTransformerObserver(nullptr);
}

void RTCFrameCryptor::OnFrameCryptionStateChanged(
    const std::string participant_id,
    webrtc::FrameCryptionState error) {
  {
    RTCFrameCryptionState state = RTCFrameCryptionState::kNew;
    switch (error) {
      case webrtc::FrameCryptionState::kNew:
        state = RTCFrameCryptionState::kNew;
        break;
      case webrtc::FrameCryptionState::kOk:
        state = RTCFrameCryptionState::kOk;
        break;
      case webrtc::FrameCryptionState::kDecryptionFailed:
        state = RTCFrameCryptionState::kDecryptionFailed;
        break;
      case webrtc::FrameCryptionState::kEncryptionFailed:
        state = RTCFrameCryptionState::kEncryptionFailed;
        break;
      case webrtc::FrameCryptionState::kMissingKey:
        state = RTCFrameCryptionState::kMissingKey;
        break;
      case webrtc::FrameCryptionState::kKeyRatcheted:
        state = RTCFrameCryptionState::kKeyRatcheted;
        break;
      case webrtc::FrameCryptionState::kInternalError:
        state = RTCFrameCryptionState::kInternalError;
        break;
    }
    webrtc::MutexLock lock(&mutex_);
    if (observer_) {
      observer_->OnFrameCryptionStateChanged(participant_id_, state);
    }
  }
}

bool RTCFrameCryptor::enabled() const {
  webrtc::MutexLock lock(&mutex_);
  return enabled_;
}

bool RTCFrameCryptor::SetKeyIndex(int index) {
  webrtc::MutexLock lock(&mutex_);
  key_index_ = index;
  e2ee_transformer_->SetKeyIndex(key_index_);
  return true;
}

int RTCFrameCryptor::key_index() const {
  webrtc::MutexLock lock(&mutex_);
  return key_index_;
}

}  // namespace libwebrtc
