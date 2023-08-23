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

#include <stdint.h>

#include <memory>
#include <string>
#include <vector>

#include "api/crypto/frame_crypto_transformer.h"
#include "livekit/rtp_receiver.h"
#include "livekit/rtp_sender.h"
#include "rtc_base/synchronization/mutex.h"
#include "rust/cxx.h"

namespace livekit {

struct KeyProviderOptions;
enum class Algorithm : ::std::int32_t;
class RTCFrameCryptorObserverWrapper;
class NativeFrameCryptorObserver;

/// Shared secret key for frame encryption.
class KeyProvider {
 public:
  KeyProvider(KeyProviderOptions options);
  ~KeyProvider() {}
  
  /// Set the key at the given index.
  bool set_key(const ::rust::String participant_id,
               int32_t index,
               rust::Vec<::std::uint8_t> key) const {
    std::vector<uint8_t> key_vec;
    std::copy(key.begin(), key.end(), std::back_inserter(key_vec));
    return impl_->SetKey(
        std::string(participant_id.data(), participant_id.size()), index,
        key_vec);
  }

  rust::Vec<::std::uint8_t> ratchet_key(const ::rust::String participant_id,
                                        int32_t key_index) const {
    rust::Vec<uint8_t> vec;
    auto data = impl_->RatchetKey(
        std::string(participant_id.data(), participant_id.size()), key_index);
    std::move(data.begin(), data.end(), std::back_inserter(vec));
    return vec;
  }

  rust::Vec<::std::uint8_t> export_key(const ::rust::String participant_id,
                                       int32_t key_index) const {
    rust::Vec<uint8_t> vec;
    auto data = impl_->ExportKey(
        std::string(participant_id.data(), participant_id.size()), key_index);
    std::move(data.begin(), data.end(), std::back_inserter(vec));
    return vec;
  }

  rtc::scoped_refptr<webrtc::KeyProvider> rtc_key_provider() { return impl_; }

 private:
  rtc::scoped_refptr<webrtc::DefaultKeyProviderImpl> impl_;
};

class FrameCryptor {
 public:
  FrameCryptor(const std::string participant_id,
               webrtc::FrameCryptorTransformer::Algorithm algorithm,
               rtc::scoped_refptr<webrtc::KeyProvider> key_provider,
               rtc::scoped_refptr<webrtc::RtpSenderInterface> sender);

  FrameCryptor(const std::string participant_id,
               webrtc::FrameCryptorTransformer::Algorithm algorithm,
               rtc::scoped_refptr<webrtc::KeyProvider> key_provider,
               rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver);
  ~FrameCryptor();

  /// Enable/Disable frame crypto for the sender or receiver.
  void set_enabled(bool enabled) const;

  /// Get the enabled state for the sender or receiver.
  bool enabled() const;

  /// Set the key index for the sender or receiver.
  /// If the key index is not set, the key index will be set to 0.
  void set_key_index(int32_t index) const;

  /// Get the key index for the sender or receiver.
  int32_t key_index() const;

  rust::String participant_id() const { return participant_id_; }

  void register_observer(rust::Box<RTCFrameCryptorObserverWrapper> observer) const;

  void unregister_observer() const;

 private:
  const rust::String participant_id_;
  mutable webrtc::Mutex mutex_;
  int32_t key_index_;
  rtc::scoped_refptr<webrtc::FrameCryptorTransformer> e2ee_transformer_;
  rtc::scoped_refptr<webrtc::KeyProvider> key_provider_;
  rtc::scoped_refptr<webrtc::RtpSenderInterface> sender_;
  rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver_;
  mutable std::unique_ptr<NativeFrameCryptorObserver> observer_;
};

class NativeFrameCryptorObserver
    : public webrtc::FrameCryptorTransformerObserver {
 public:
  NativeFrameCryptorObserver(rust::Box<RTCFrameCryptorObserverWrapper> observer,
                             const FrameCryptor* fc);

  void OnFrameCryptionStateChanged(const std::string participant_id,
                                   webrtc::FrameCryptionState error) override;

 private:
  rust::Box<RTCFrameCryptorObserverWrapper> observer_;
  const FrameCryptor* fc_;
};

std::shared_ptr<FrameCryptor> new_frame_cryptor_for_rtp_sender(
    const ::rust::String participant_id,
    Algorithm algorithm,
    std::shared_ptr<KeyProvider> key_provider,
    std::shared_ptr<RtpSender> sender);

std::shared_ptr<FrameCryptor> new_frame_cryptor_for_rtp_receiver(
    const ::rust::String participant_id,
    Algorithm algorithm,
    std::shared_ptr<KeyProvider> key_provider,
    std::shared_ptr<RtpReceiver> receiver);

std::shared_ptr<KeyProvider> new_key_provider(KeyProviderOptions options);

}  // namespace livekit
