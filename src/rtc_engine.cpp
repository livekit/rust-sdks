//
// Created by Théo Monnom on 04/05/2022.
//

#include "rtc_engine.h"
#include <rtc_base/ssl_adapter.h>
#include <spdlog/spdlog.h>
#include "peer_transport.h"

namespace livekit{

    RTCEngine::RTCEngine() {

    }

    void RTCEngine::Join(const std::string &url, const std::string &token){
        client_.Connect(url, token);
    }

    void RTCEngine::Update(){
        client_.update();

        auto res = client_.poll();
        if(res.has_join())
            OnJoin(res.join());
    }

    void RTCEngine::OnJoin(const JoinResponse &res){
        spdlog::info("OnJoin");
        rtc::InitializeSSL();

        for(auto& is : res.ice_servers()){
            webrtc::PeerConnectionInterface::IceServer ice_server;
            for(auto& url : is.urls())
                ice_server.urls.push_back(url);

            ice_server.username = is.username();
            ice_server.password = is.credential();

            configuration_.servers.push_back(ice_server);
        }

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
            // TODO Make error callback
            spdlog::error("Error on CreateModularPeerConnectionFactory");
            return;
        }

        subscriber_ = std::make_unique<PeerTransport>(*this);
        publisher_ = std::make_unique<PeerTransport>(*this);
    }

    void RTCEngine::Configure() {

    }


} // livekit
