//
// Created by theom on 04/09/2022.
//

#include "livekit/rtc_error.h"

#include <iomanip>
#include <sstream>
#include <string>

namespace livekit {

RTCError to_error(const webrtc::RTCError& error) {
  RTCError lk_error;
  lk_error.error_detail = static_cast<RTCErrorDetailType>(error.error_detail());
  lk_error.error_type = static_cast<RTCErrorType>(error.type());
  lk_error.has_sctp_cause_code = error.sctp_cause_code().has_value();
  lk_error.sctp_cause_code = error.sctp_cause_code().value_or(0);
  lk_error.message = error.message();
  return lk_error;
}

std::string serialize_error(const RTCError& error) {
  std::stringstream ss;
  ss << std::hex << std::setfill('0');
  ss << std::setw(8) << (uint32_t)error.error_type;
  ss << std::setw(8) << (uint32_t)error.error_detail;
  ss << std::setw(2) << (uint16_t)error.has_sctp_cause_code;
  ss << std::setw(4) << (uint16_t)error.sctp_cause_code;
  ss << std::dec << std::setw(1) << std::string(error.message);
  return ss.str();
}

#ifdef LIVEKIT_TEST
rust::String serialize_deserialize() {
  RTCError lk_error;
  lk_error.error_type = RTCErrorType::InternalError;
  lk_error.error_detail = RTCErrorDetailType::DataChannelFailure;
  lk_error.has_sctp_cause_code = true;
  lk_error.sctp_cause_code = 24;
  lk_error.message = "this is not a test, I repeat, this is not a test";
  return serialize_error(lk_error);
}

void throw_error() {
  RTCError lk_error;
  lk_error.error_type = RTCErrorType::InvalidModification;
  lk_error.error_detail = RTCErrorDetailType::None;
  lk_error.has_sctp_cause_code = false;
  lk_error.sctp_cause_code = 0;
  lk_error.message = "exception is thrown!";
  throw std::runtime_error(serialize_error(lk_error));
}
#endif

}  // namespace livekit
