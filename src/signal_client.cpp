//
// Created by Théo Monnom on 27/04/2022.
//

#include "signal_client.h"
#include "proto/livekit_rtc.pb.h"
#include <boost/regex.hpp>
#include <iostream>

namespace livekit {
    void SignalClient::Connect(const std::string& url, const std::string& token) {
        boost::regex reg("(ws|wss)://([^:]+):?([^/]*)(.*)");
        boost::match_results<std::string::const_iterator> regMatches;
        if (boost::regex_match(url, regMatches, reg))
        {
            const std::string protocol = regMatches[1]; // TODO Check if secure or not
            const std::string domain = regMatches[2];
            const std::string port = regMatches[3];

            m_WebSocket = std::make_unique<websocket::stream<tcp::socket>>(m_IOContext);

            tcp::resolver resolver{m_IOContext};
            auto const results = resolver.resolve(domain, port);
            net::connect(m_WebSocket->next_layer(), results.begin(), results.end());

            m_WebSocket->handshake(domain, "/rtc?access_token=" + token);

            while(true){
                beast::flat_buffer buffer;
                m_WebSocket->read(buffer);
            }

        }else{
            throw std::runtime_error{"Failed to parse url"};
        }
    }

}