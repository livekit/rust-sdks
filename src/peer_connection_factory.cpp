//
// Created by Théo Monnom on 03/08/2022.
//

#include "peer_connection_factory.h"
#include <iostream>

namespace lk{

    PeerConnectionFactory::PeerConnectionFactory(){
        rtc::LogMessage::LogToDebug(rtc::LS_INFO);
        RTC_LOG(LS_INFO) << "PeerConnectionFactory::PeerConnectionFactory()";

        network_thread_ = rtc::Thread::CreateWithSocketServer();
        network_thread_->Start();
        worker_thread_ = rtc::Thread::Create();
        worker_thread_->Start();
        signaling_thread_ = rtc::Thread::Create();
        signaling_thread_->Start();

        webrtc::PeerConnectionFactoryDependencies dependencies;
        dependencies.network_thread = network_thread_.get();
        dependencies.worker_thread = worker_thread_.get();
        dependencies.signaling_thread = signaling_thread_.get();
        peer_factory_ = webrtc::CreateModularPeerConnectionFactory(std::move(dependencies));

        if (peer_factory_.get() == nullptr) {
            RTC_LOG_ERR(LS_ERROR) << "Failed to create PeerConnectionFactory";
            return;
        }

        // TODO(theomonnom) Close resources
    }

    std::unique_ptr<PeerConnectionFactory> CreatePeerConnectionFactory() {
        return std::make_unique<PeerConnectionFactory>();
    }

} // namespace lk