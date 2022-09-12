//
// Created by Théo Monnom on 01/09/2022.
//

#include "livekit/data_channel.h"

namespace livekit {

    DataChannel::DataChannel(rtc::scoped_refptr<webrtc::DataChannelInterface> data_channel) : data_channel_(data_channel) {

    }
} // livekit