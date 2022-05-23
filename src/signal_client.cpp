//
// Created by Théo Monnom on 27/04/2022.
//

#include "signal_client.h"
#include <thread>
#include <spdlog/spdlog.h>

namespace livekit {

    SignalClient::SignalClient() : m_Connected(false), m_Writing(false), m_Reading(false) {

    }

    SignalClient::~SignalClient() {
        Disconnect();
    }

    void SignalClient::Connect(const std::string &url, const std::string &token) {
        if (m_Connected)
            throw std::runtime_error{"already connected"};

        m_URL = ParseURL(url);
        m_Token = token;

        start(); // We don't need a thread, everything is async ( + easier to maintain )
    }

    void SignalClient::update() {
        beast::error_code ec;
        m_IOContext.poll(ec);

        if (ec)
            throw std::runtime_error{"SignalClient::Update - " + ec.message()};

        if (m_Connected) {
            if (!m_WebSocket.is_open())
                throw std::runtime_error{"Websocket isn't open"}; // TODO Start reconnect

            if (!m_Reading) {
                m_WebSocket.async_read(m_ReadBuffer, beast::bind_front_handler(&SignalClient::OnRead, this));
                m_Reading = true;
            }

            // Write pending messages
            if (!m_Writing && !m_WriteQueue.empty()) {
                auto req = m_WriteQueue.front();

                unsigned long len = req.ByteSizeLong();
                uint8_t data[len];
                req.SerializeToArray(data, len);

                m_WebSocket.async_write(net::buffer(&data, len),
                                        beast::bind_front_handler(&SignalClient::OnWrite, this));

                m_WriteQueue.pop();
                m_Writing = true;
            }
        }
    }

    SignalResponse SignalClient::poll(){


        auto& r = m_ReadQueue.front();
        m_ReadQueue.pop();
        return r;
    }

    void SignalClient::start() {
        m_Resolver.async_resolve(m_URL.host, m_URL.port, beast::bind_front_handler(&SignalClient::OnResolve, this));
    }

    void SignalClient::Disconnect() {
        if (!m_Connected)
            return;

        m_Connected = false;
        m_Work.reset();
        //m_IOContext.stop();
        m_WebSocket.close(websocket::close_code::normal); // TODO Close should be async
    }

    void SignalClient::Send(SignalRequest req) {
        m_WriteQueue.emplace(req);
    }

    void SignalClient::OnResolve(beast::error_code ec, tcp::resolver::results_type results) {
        if (ec)
            throw std::runtime_error{"SignalClient::OnResolve - " + ec.message()};

        auto &layer = beast::get_lowest_layer(m_WebSocket);
        layer.expires_after(std::chrono::seconds(15));
        layer.async_connect(results, beast::bind_front_handler(&SignalClient::OnConnect, this));
    }

    void SignalClient::OnConnect(beast::error_code ec, tcp::resolver::results_type::endpoint_type ep) {
        if (ec)
            throw std::runtime_error{"SignalClient::OnConnect - " + ec.message()};

        beast::get_lowest_layer(m_WebSocket).expires_never();
        m_WebSocket.set_option(websocket::stream_base::timeout::suggested(beast::role_type::client));

        m_WebSocket.async_handshake(m_URL.host, "/rtc?access_token=" + m_Token + "&protocol=7",
                                    beast::bind_front_handler(&SignalClient::OnHandshake, this));
    }

    void SignalClient::OnHandshake(beast::error_code ec) {
        if (ec)
            throw std::runtime_error{
                    "SignalClient::OnHandshake - " + ec.message()}; // TODO Callback for handling errors

        m_Connected = true;
        spdlog::info("Connected to Websocket");
    }

    void SignalClient::OnRead(beast::error_code ec, std::size_t bytesTransferred) {
        m_Reading = false;

        if (ec)
            throw std::runtime_error{"SignalClient::OnRead - " + ec.message()};

        SignalResponse res{};
        if (res.ParseFromArray(m_ReadBuffer.cdata().data(), bytesTransferred)) {
            m_ReadQueue.emplace(res);
        } else {
            spdlog::error("Failed to decode signal message");
        }

        m_ReadBuffer.clear();
    }

    void SignalClient::OnWrite(beast::error_code ec, std::size_t bytesTransferred) {
        m_Writing = false;

        if (ec)
            throw std::runtime_error{"SignalClient::OnWrite - " + ec.message()};
    }
} // livekit