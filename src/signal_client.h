//
// Created by Théo Monnom on 27/04/2022.
//

#ifndef LIVEKIT_NATIVE_SIGNAL_CLIENT_H
#define LIVEKIT_NATIVE_SIGNAL_CLIENT_H

#include <boost/beast/core.hpp>
#include <boost/beast/websocket.hpp>
#include <queue>
#include "proto/livekit_rtc.pb.h"
#include "utils.h"

namespace beast = boost::beast;         // from <boost/beast.hpp>
namespace http = beast::http;           // from <boost/beast/http.hpp>
namespace websocket = beast::websocket; // from <boost/beast/websocket.hpp>
namespace net = boost::asio;            // from <boost/asio.hpp>
using tcp = boost::asio::ip::tcp;       // from <boost/asio/ip/tcp.hpp>

// If we keep the code singled threaded here, it'll be easily used in wasm ( Need ws bindings ), tho not sure
namespace livekit {
    class SignalClient {

    public:
        SignalClient();
        ~SignalClient();

        void Connect(const std::string &url, const std::string &token);
        void Disconnect();
        void update();
        void Send(SignalRequest req);
        SignalResponse poll();

    private:
        void start();

        // beast handlers
        void OnResolve(beast::error_code ec, tcp::resolver::results_type results);
        void OnConnect(beast::error_code ec, tcp::resolver::results_type::endpoint_type ep);
        void OnHandshake(beast::error_code ec);
        void OnRead(beast::error_code ec, std::size_t bytesTransferred);
        void OnWrite(beast::error_code ec, std::size_t bytesTransferred);

    private:
        URL m_URL;
        std::string m_Token;

        std::queue<SignalResponse> m_ReadQueue;
        std::queue<SignalRequest> m_WriteQueue;
        bool m_Connected;
        bool m_Reading, m_Writing;

        beast::flat_buffer m_WriteBuffer;
        beast::flat_buffer m_ReadBuffer;

        // Keep order
        net::io_context m_IOContext;
        net::executor_work_guard <net::io_context::executor_type> m_Work = net::make_work_guard(
                m_IOContext); // Prevent the IOContext from running out of work
        tcp::resolver m_Resolver{m_IOContext};
        websocket::stream <beast::tcp_stream> m_WebSocket{m_IOContext};
    };
} // livekit

#endif //LIVEKIT_NATIVE_SIGNAL_CLIENT_H