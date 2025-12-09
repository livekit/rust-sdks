#ifndef LIVEKIT_UTILS_H
#define LIVEKIT_UTILS_H

#include "api/peer_connection_interface.h"
#include "api/rtc_error.h"
#include "livekit_rtc/capi.h"
#include "rtc_base/logging.h"

namespace livekit {

lkRtcError toRtcError(const webrtc::RTCError& error);

webrtc::PeerConnectionInterface::RTCOfferAnswerOptions
    toNativeOfferAnswerOptions(const lkOfferAnswerOptions& options);

class LKString : public webrtc::RefCountInterface {
 public:
  explicit LKString(const std::string& str) : str_(str) {}
  ~LKString() { RTC_LOG(LS_INFO) << "LKString destroyed"; }

  std::string get() const { return str_; }

  size_t length() const { return str_.size(); }

  const uint8_t* data() const {
    return reinterpret_cast<const uint8_t*>(str_.data());
  }

 private:
  std::string str_;
};

class LKData : public webrtc::RefCountInterface {
 public:
  explicit LKData() = default;

  explicit LKData(const std::vector<uint8_t>& data) : data_(data) {}

  ~LKData() { RTC_LOG(LS_INFO) << "LKData destroyed"; }

  static webrtc::scoped_refptr<LKData> FromRaw(const uint8_t* data,
                                               size_t size) {
    std::vector<uint8_t> vec(data, data + size);
    return webrtc::make_ref_counted<LKData>(vec);
  }

  std::vector<uint8_t> get() const { return data_; }

  size_t size() const { return data_.size(); }

  uint8_t get_at(size_t index) const { return data_.at(index); }

  void push_back(const uint8_t& value) { data_.push_back(value); }

  const uint8_t* data() const { return data_.data(); }

 private:
  std::vector<uint8_t> data_;
};

template <typename T>
class LKVector : public webrtc::RefCountInterface {
 public:
  explicit LKVector() = default;

  explicit LKVector(const std::vector<T>& vec) : vec_(vec) {}

  ~LKVector() { RTC_LOG(LS_INFO) << "LKVector destroyed"; }

  std::vector<T> get() const { return vec_; }

  size_t size() const { return vec_.size(); }

  T get_at(size_t index) const { return vec_.at(index); }

  void push_back(const T& value) { vec_.push_back(value); }

 private:
  std::vector<T> vec_;
};

}  // namespace livekit

#endif  // LIVEKIT_UTILS_H
