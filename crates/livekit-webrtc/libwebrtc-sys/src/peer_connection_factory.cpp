//
// Created by Théo Monnom on 03/08/2022.
//

#include "livekit/peer_connection_factory.h"
#include "api/video_codecs/builtin_video_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/video_codecs/builtin_video_encoder_factory.h"
#include "media/engine/webrtc_media_engine.h"
#include "api/task_queue/default_task_queue_factory.h"
#include "api/rtc_event_log/rtc_event_log_factory.h"
#include "libwebrtc-sys/src/peer_connection_factory.rs.h"
#include "livekit/rtc_error.h"

namespace livekit{

    PeerConnectionFactory::PeerConnectionFactory(){
        rtc::LogMessage::LogToDebug(rtc::LS_INFO);
        RTC_LOG(LS_INFO) << "PeerConnectionFactory::PeerConnectionFactory()";

        network_thread_ = rtc::Thread::CreateWithSocketServer();
        network_thread_->SetName("network_thread", &network_thread_);
        network_thread_->Start();
        worker_thread_ = rtc::Thread::Create();
        worker_thread_->SetName("worker_thread", &worker_thread_);
        worker_thread_->Start();
        signaling_thread_ = rtc::Thread::Create();
        signaling_thread_->SetName("signaling_thread", &signaling_thread_);
        signaling_thread_->Start();

        webrtc::PeerConnectionFactoryDependencies dependencies;
        dependencies.network_thread = network_thread_.get();
        dependencies.worker_thread = worker_thread_.get();
        dependencies.signaling_thread = signaling_thread_.get();
        dependencies.task_queue_factory = webrtc::CreateDefaultTaskQueueFactory();
        dependencies.event_log_factory = std::make_unique<webrtc::RtcEventLogFactory>(dependencies.task_queue_factory.get());

        cricket::MediaEngineDependencies media_deps;
        media_deps.task_queue_factory = dependencies.task_queue_factory.get();
        media_deps.video_encoder_factory = webrtc::CreateBuiltinVideoEncoderFactory();
        media_deps.video_decoder_factory = webrtc::CreateBuiltinVideoDecoderFactory();
        media_deps.audio_encoder_factory = webrtc::CreateBuiltinAudioEncoderFactory();
        media_deps.audio_decoder_factory = webrtc::CreateBuiltinAudioDecoderFactory();

        dependencies.media_engine = cricket::CreateMediaEngine(std::move(media_deps));

        peer_factory_ = webrtc::CreateModularPeerConnectionFactory(std::move(dependencies));

        if (peer_factory_.get() == nullptr) {
            RTC_LOG_ERR(LS_ERROR) << "Failed to create PeerConnectionFactory";
            return;
        }
    }

    std::unique_ptr<PeerConnection> PeerConnectionFactory::create_peer_connection(std::unique_ptr<webrtc::PeerConnectionInterface::RTCConfiguration> config, std::unique_ptr<NativePeerConnectionObserver> observer) const {
        webrtc::PeerConnectionDependencies deps{observer.get()};
        auto result = peer_factory_->CreatePeerConnectionOrError(*config, std::move(deps));

        if(!result.ok()){
            throw std::runtime_error(serialize_error(to_error(result.error())));
        }

        return std::make_unique<PeerConnection>(std::move(result.value()), std::move(observer));
    }

    std::unique_ptr<PeerConnectionFactory> create_peer_connection_factory() {
        return std::make_unique<PeerConnectionFactory>();
    }

    std::unique_ptr<NativeRTCConfiguration> create_rtc_configuration(RTCConfiguration conf){
        auto rtc = std::make_unique<webrtc::PeerConnectionInterface::RTCConfiguration>();

        for (auto &item: conf.ice_servers){
            webrtc::PeerConnectionInterface::IceServer ice_server;
            ice_server.username = item.username.c_str();
            ice_server.password = item.password.c_str();

            for (auto &url: item.urls){
                ice_server.urls.emplace_back(url.c_str());
            }

            rtc->servers.push_back(ice_server);
        }

        return rtc;
    }
} // namespace lk