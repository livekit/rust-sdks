//
// Created by Théo Monnom on 01/09/2022.
//

#ifndef CLIENT_SDK_NATIVE_DATA_CHANNEL_H
#define CLIENT_SDK_NATIVE_DATA_CHANNEL_H

#include <memory>
#include "api/data_channel_interface.h"
#include "rust_types.h"

namespace livekit {
    using NativeDataChannelInit = webrtc::DataChannelInit;

    class DataChannel {
    public:
        explicit DataChannel(rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel);

    private:
        rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel_;
    };

    std::unique_ptr<NativeDataChannelInit> create_data_channel_init(DataChannelInit init);

    static std::unique_ptr<DataChannel> _unique_data_channel(){
        return nullptr; // Ignore
    }

} // livekit

#endif //CLIENT_SDK_NATIVE_DATA_CHANNEL_H
