//
// Created by Théo Monnom on 27/04/2022.
//

#ifndef LIVEKIT_NATIVE_SIGNAL_CLIENT_H
#define LIVEKIT_NATIVE_SIGNAL_CLIENT_H

#include <boost/beast/core.hpp>
#include <boost/beast/websocket.hpp>

namespace beast = boost::beast;         // from <boost/beast.hpp>
namespace http = beast::http;           // from <boost/beast/http.hpp>
namespace websocket = beast::websocket; // from <boost/beast/websocket.hpp>
namespace net = boost::asio;            // from <boost/asio.hpp>
using tcp = boost::asio::ip::tcp;       // from <boost/asio/ip/tcp.hpp>

namespace livekit {
    class SignalClient {

    public:
        void Connect(const std::string& url, const std::string& token);

    private:
        net::io_context m_IOContext;
        std::unique_ptr<websocket::stream<tcp::socket>> m_WebSocket;
    };
}

#endif //LIVEKIT_NATIVE_SIGNAL_CLIENT_H