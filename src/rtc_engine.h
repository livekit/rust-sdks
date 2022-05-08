//
// Created by Théo Monnom on 04/05/2022.
//

#ifndef LIVEKIT_NATIVE_RTC_ENGINE_H
#define LIVEKIT_NATIVE_RTC_ENGINE_H

#include "signal_client.h"

namespace livekit{

    class RTCEngine {


    private:
        void configure();

    private:
        SignalClient m_Client;
    };
};



#endif //LIVEKIT_NATIVE_RTC_ENGINE_H
