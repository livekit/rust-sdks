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

#pragma once

#include "api/crypto/frame_crypto_transformer.h"
#include "api/rtp_receiver_interface.h"
#include "api/rtp_sender_interface.h"

#include "rust/cxx.h"
#include <stdint.h>

#include <vector>
#include <string>
#include <memory>

namespace livekit {

enum class Algorithm {
  kAesGcm = 0,
  kAesCbc,
};

struct KeyProviderOptions {
  bool shared_key;
  std::vector<uint8_t> ratchet_salt;
  std::vector<uint8_t> uncrypted_magic_bytes;
  int ratchet_window_size;
  KeyProviderOptions()
      : shared_key(false),
        ratchet_salt(std::vector<uint8_t>()),
        ratchet_window_size(0) {}
  KeyProviderOptions(KeyProviderOptions& copy)
      : shared_key(copy.shared_key),
        ratchet_salt(copy.ratchet_salt),
        ratchet_window_size(copy.ratchet_window_size) {}
};


/// Shared secret key for frame encryption.
class KeyProvider {
 public:
  KeyProvider(KeyProviderOptions* options) {
    webrtc::KeyProviderOptions rtc_options;
    rtc_options.shared_key = options->shared_key;
    rtc_options.ratchet_salt = options->ratchet_salt;
    rtc_options.uncrypted_magic_bytes =
        options->uncrypted_magic_bytes;
    rtc_options.ratchet_window_size = options->ratchet_window_size;
    impl_ =
        new rtc::RefCountedObject<webrtc::DefaultKeyProviderImpl>(rtc_options);
  }
  ~KeyProvider() {}
  /// Set the key at the given index.
  bool SetKey(const std::string participant_id,
              int index,
              std::vector<uint8_t> key)  {
    return impl_->SetKey(participant_id, index, key);
  }

  std::vector<uint8_t> RatchetKey(const std::string participant_id,
                             int key_index)  {
    return impl_->RatchetKey(participant_id, key_index);
  }

  std::vector<uint8_t> ExportKey(const std::string participant_id,
                            int key_index) {
    return impl_->ExportKey(participant_id, key_index);
  }

  rtc::scoped_refptr<webrtc::KeyProvider> rtc_key_provider() { return impl_; }

 private:
  rtc::scoped_refptr<webrtc::DefaultKeyProviderImpl> impl_;
};


enum RTCFrameCryptionState {
  kNew = 0,
  kOk,
  kEncryptionFailed,
  kDecryptionFailed,
  kMissingKey,
  kKeyRatcheted,
  kInternalError,
};

class RTCFrameCryptorObserver {
 public:
  virtual void OnFrameCryptionStateChanged(const std::string participant_id,
                                           RTCFrameCryptionState state) = 0;

 protected:
  virtual ~RTCFrameCryptorObserver() {}
};

class RTCFrameCryptor : public webrtc::FrameCryptorTransformerObserver, public rtc::RefCountInterface {
 public:
  RTCFrameCryptor(const std::string participant_id,
                      Algorithm algorithm,
                      rtc::scoped_refptr<webrtc::KeyProvider> key_provider,
                      rtc::scoped_refptr<webrtc::RtpSenderInterface> sender);

  RTCFrameCryptor(const std::string participant_id,
                      Algorithm algorithm,
                      rtc::scoped_refptr<webrtc::KeyProvider> key_provider,
                      rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver);
  ~RTCFrameCryptor();

  /// Enable/Disable frame crypto for the sender or receiver.
  bool SetEnabled(bool enabled) ;

  /// Get the enabled state for the sender or receiver.
  bool enabled() const ;

  /// Set the key index for the sender or receiver.
  /// If the key index is not set, the key index will be set to 0.
  bool SetKeyIndex(int index) ;

  /// Get the key index for the sender or receiver.
  int key_index() const ;

  const std::string participant_id() const  { return participant_id_; }

  void RegisterRTCFrameCryptorObserver(
      RTCFrameCryptorObserver* observer) ;

  void DeRegisterRTCFrameCryptorObserver() ;

  void OnFrameCryptionStateChanged(const std::string participant_id,
                                   webrtc::FrameCryptionState error) ;

 private:
  std::string participant_id_;
  mutable webrtc::Mutex mutex_;
  bool enabled_;
  int key_index_;
  rtc::scoped_refptr<webrtc::FrameCryptorTransformer> e2ee_transformer_;
  rtc::scoped_refptr<webrtc::KeyProvider> key_provider_;
  rtc::scoped_refptr<webrtc::RtpSenderInterface> sender_;
  rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver_;
  RTCFrameCryptorObserver* observer_ = nullptr;
};

class FrameCryptorFactory {
 public:
  /// Create a frame cyrptor for [RTCRtpSender].
  static rtc::scoped_refptr<RTCFrameCryptor>
  frameCryptorFromRtpSender(const std::string participant_id,
                            rtc::scoped_refptr<webrtc::RtpSenderInterface> sender,
                            Algorithm algorithm,
                            rtc::scoped_refptr<webrtc::KeyProvider> key_provider);

  /// Create a frame cyrptor for [RTCRtpReceiver].
  static rtc::scoped_refptr<RTCFrameCryptor>
  frameCryptorFromRtpReceiver(const std::string participant_id,
                              rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver,
                              Algorithm algorithm,
                              rtc::scoped_refptr<webrtc::KeyProvider> key_provider);
};

}  // namespace livekit