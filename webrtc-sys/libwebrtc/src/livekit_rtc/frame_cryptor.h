/*
 * Copyright 2025 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#pragma once

#include <stdint.h>

#include <memory>
#include <string>
#include <vector>

#include "livekit_rtc/include/capi.h"
#include "api/crypto/frame_crypto_transformer.h"
#include "api/scoped_refptr.h"
#include "livekit_rtc/peer.h"
#include "livekit_rtc/rtp_transceiver.h"
#include "rtc_base/synchronization/mutex.h"

namespace livekit {

typedef struct {
  bool shared_key;
  int32_t ratchet_window_size;
  std::vector<uint8_t> ratchet_salt;
  bool failure_tolerance;
} KeyProviderOptions;

typedef struct {
  std::vector<uint8_t> data;
  std::vector<uint8_t> iv;
  uint32_t key_index;
} EncryptedPacket;

enum Algorithm {
  AesGcm = 0,
  AesCbc,
};

enum FrameCryptionState {
  kNew,
  kOk,
  kEncryptionFailed,
  kDecryptionFailed,
  kMissingKey,
  kKeyRatcheted,
  kInternalError,
};

using RtcFrameCryptorObserverWrapper = void (*)(const char* participant_id,
                                                FrameCryptionState state,
                                                const lkFrameCryptor* fc,
                                                void* userdata);

/// Shared secret key for frame encryption.
class KeyProvider {
 public:
  KeyProvider(KeyProviderOptions options);
  ~KeyProvider() {}

  bool set_shared_key(int32_t index, std::vector<::std::uint8_t> key) const {
    std::vector<uint8_t> key_vec;
    std::copy(key.begin(), key.end(), std::back_inserter(key_vec));
    return impl_->SetSharedKey(index, key_vec);
  }

  std::vector<::std::uint8_t> ratchet_shared_key(int32_t key_index) const {
    std::vector<uint8_t> vec;
    auto data = impl_->RatchetSharedKey(key_index);
    if (data.empty()) {
      // TODO: throw std::runtime_error("ratchet_shared_key failed");
    }

    std::move(data.begin(), data.end(), std::back_inserter(vec));
    return vec;
  }

  std::vector<::std::uint8_t> get_shared_key(int32_t key_index) const {
    std::vector<uint8_t> vec;
    auto data = impl_->ExportSharedKey(key_index);
    if (data.empty()) {
      // TODO: throw std::runtime_error("get_shared_key failed");
    }

    std::move(data.begin(), data.end(), std::back_inserter(vec));
    return vec;
  }

  /// Set the key at the given index.
  bool set_key(const std::string participant_id,
               int32_t index,
               std::vector<::std::uint8_t> key) const {
    std::vector<uint8_t> key_vec;
    std::copy(key.begin(), key.end(), std::back_inserter(key_vec));
    return impl_->SetKey(std::string(participant_id.data(), participant_id.size()), index, key_vec);
  }

  std::vector<::std::uint8_t> ratchet_key(const std::string participant_id,
                                          int32_t key_index) const {
    std::vector<uint8_t> vec;
    auto data =
        impl_->RatchetKey(std::string(participant_id.data(), participant_id.size()), key_index);
    if (data.empty()) {
      // TODO: throw std::runtime_error("ratchet_key failed");
    }

    std::move(data.begin(), data.end(), std::back_inserter(vec));
    return vec;
  }

  std::vector<::std::uint8_t> get_key(const std::string participant_id, int32_t key_index) const {
    std::vector<uint8_t> vec;
    auto data =
        impl_->ExportKey(std::string(participant_id.data(), participant_id.size()), key_index);
    if (data.empty()) {
      // TODO: throw std::runtime_error("get_key failed");
    }

    std::move(data.begin(), data.end(), std::back_inserter(vec));
    return vec;
  }

  void set_sif_trailer(std::vector<::std::uint8_t> trailer) const {
    std::vector<uint8_t> trailer_vec;
    std::copy(trailer.begin(), trailer.end(), std::back_inserter(trailer_vec));
    impl_->SetSifTrailer(trailer_vec);
  }

  webrtc::scoped_refptr<webrtc::KeyProvider> rtc_key_provider() { return impl_; }

 private:
  webrtc::scoped_refptr<webrtc::DefaultKeyProviderImpl> impl_;
};

class NativeFrameCryptorObserver;

class FrameCryptor : public webrtc::RefCountInterface {
 public:
  FrameCryptor(webrtc::Thread* thread,
               const std::string participant_id,
               webrtc::FrameCryptorTransformer::Algorithm algorithm,
               webrtc::scoped_refptr<webrtc::KeyProvider> key_provider,
               webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender);

  FrameCryptor(webrtc::Thread* thread,
               const std::string participant_id,
               webrtc::FrameCryptorTransformer::Algorithm algorithm,
               webrtc::scoped_refptr<webrtc::KeyProvider> key_provider,
               webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver);
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

  std::string participant_id() const { return participant_id_; }

  void register_observer(RtcFrameCryptorObserverWrapper observer, void* userdata);

  void unregister_observer() const;

 private:
  webrtc::Thread* thread_;
  const std::string participant_id_;
  mutable webrtc::Mutex mutex_;
  webrtc::scoped_refptr<webrtc::FrameCryptorTransformer> e2ee_transformer_;
  webrtc::scoped_refptr<webrtc::KeyProvider> key_provider_;
  webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender_;
  webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver_;
  mutable webrtc::scoped_refptr<NativeFrameCryptorObserver> observer_;
};

class NativeFrameCryptorObserver : public webrtc::FrameCryptorTransformerObserver {
 public:
  NativeFrameCryptorObserver(RtcFrameCryptorObserverWrapper observer,
                             const FrameCryptor* fc,
                             void* userdata);
  ~NativeFrameCryptorObserver();

  void OnFrameCryptionStateChanged(const std::string participant_id,
                                   webrtc::FrameCryptionState error) override;

 private:
  RtcFrameCryptorObserverWrapper observer_;
  const FrameCryptor* fc_;
  void* userdata_;
};

class DataPacketCryptor : public webrtc::RefCountInterface {
 public:
  DataPacketCryptor(webrtc::FrameCryptorTransformer::Algorithm algorithm,
                    webrtc::scoped_refptr<webrtc::KeyProvider> key_provider);

  EncryptedPacket encrypt_data_packet(const std::string participant_id,
                                      uint32_t key_index,
                                      std::vector<::std::uint8_t> data) const;

  std::vector<::std::uint8_t> decrypt_data_packet(const std::string participant_id,
                                                  const EncryptedPacket& encrypted_packet) const;

 private:
  webrtc::scoped_refptr<webrtc::DataPacketCryptor> data_packet_cryptor_;
};

}  // namespace livekit
