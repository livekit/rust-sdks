#ifndef LIVEKIT_DATA_CHANNEL_H
#define LIVEKIT_DATA_CHANNEL_H

#include "api/data_channel_interface.h"
#include "api/scoped_refptr.h"
#include "livekit_rtc/include/capi.h"
#include "livekit_rtc/utils.h"
#include "rtc_base/copy_on_write_buffer.h"
#include "rtc_base/ref_count.h"
#include "rtc_base/synchronization/mutex.h"

namespace livekit {

webrtc::DataChannelInit toNativeDataChannelInit(const lkDataChannelInit& init);
class DataChannel;

class DataChannelObserver : public webrtc::DataChannelObserver {
 public:
  DataChannelObserver(
      const lkDataChannelObserver* observer,
      webrtc::scoped_refptr<webrtc::DataChannelInterface> data_channel,
      void* userdata)
      : observer_(observer), data_channel_(data_channel), userdata_(userdata) {}

  void OnStateChange() override;

  void OnMessage(const webrtc::DataBuffer& buffer) override;

  void OnBufferedAmountChange(uint64_t sentDataSize) override;

  bool IsOkToCallOnTheNetworkThread() override { return true; }

 private:
  const lkDataChannelObserver* observer_;
  webrtc::scoped_refptr<webrtc::DataChannelInterface> data_channel_;
  void* userdata_;
};

class DataChannel : public webrtc::RefCountInterface {
 public:
  DataChannel(webrtc::scoped_refptr<webrtc::DataChannelInterface> data_channel)
      : data_channel_(data_channel) {}

  lkDcState State() const {
    return static_cast<lkDcState>(data_channel_->state());
  }

  int Id() const { return data_channel_->id(); }

  std::string label() const { return data_channel_->label(); }

  uint64_t buffered_amount() const { return data_channel_->buffered_amount(); }

  void RegisterObserver(const lkDataChannelObserver* observer, void* userdata);
  void UnregisterObserver();

  void SendAsync(const uint8_t* data,
                 uint64_t size,
                 bool binary,
                 void (*onComplete)(lkRtcError* error, void* userdata),
                 void* userdata);

  void Close() { data_channel_->Close(); }

 private:
  webrtc::Mutex mutex_;
  webrtc::scoped_refptr<webrtc::DataChannelInterface> data_channel_;
  std::unique_ptr<DataChannelObserver> observer_ = nullptr;
};

}  // namespace livekit

#endif  // LIVEKIT_DATA_CHANNEL_H
