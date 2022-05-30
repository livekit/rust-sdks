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
        URL url_;
        std::string token_;

        std::queue<SignalResponse> read_queue_;
        std::queue<SignalRequest> write_queue_;
        bool connected_;
        bool reading_, writing_;

        beast::flat_buffer write_buffer_;
        beast::flat_buffer read_buffer_;

        // Keep order
        net::io_context io_context_;
        net::executor_work_guard <net::io_context::executor_type> work_guard_ = net::make_work_guard(
                io_context_); // Prevent the IOContext from running out of work
        tcp::resolver resolver_{io_context_};
        websocket::stream <beast::tcp_stream> websocket_{io_context_};
    };
} // livekit

#endif //LIVEKIT_NATIVE_SIGNAL_CLIENT_H