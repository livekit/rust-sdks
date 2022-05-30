//
// Created by Théo Monnom on 27/04/2022.
//

#include "signal_client.h"
#include <thread>
#include <spdlog/spdlog.h>

namespace livekit {

    SignalClient::SignalClient() : connected_(false), writing_(false), reading_(false) {

    }

    SignalClient::~SignalClient() {
        Disconnect();
    }

    void SignalClient::Connect(const std::string &url, const std::string &token) {
        if (connected_)
            throw std::runtime_error{"already connected"};

        url_ = ParseURL(url);
        token_ = token;

        start(); // We don't need a thread, everything is async ( + easier to maintain )
    }

    void SignalClient::update() {
        beast::error_code ec;
        io_context_.poll(ec);

        if (ec)
            throw std::runtime_error{"SignalClient::Update - " + ec.message()};

        if (connected_) {
            if (!websocket_.is_open())
                throw std::runtime_error{"Websocket isn't open"}; // TODO Start reconnect

            if (!reading_) {
                websocket_.async_read(read_buffer_, beast::bind_front_handler(&SignalClient::OnRead, this));
                reading_ = true;
            }

            // Write pending messages
            if (!writing_ && !write_queue_.empty()) {
                auto req = write_queue_.front();

                unsigned long len = req.ByteSizeLong();
                uint8_t data[len];
                req.SerializeToArray(data, len);

                websocket_.async_write(net::buffer(&data, len),
                                        beast::bind_front_handler(&SignalClient::OnWrite, this));

                write_queue_.pop();
                writing_ = true;
            }
        }
    }

    SignalResponse SignalClient::poll(){
        if(read_queue_.empty())
            return SignalResponse{};

        auto r = read_queue_.front();
        read_queue_.pop();
        return r;
    }

    void SignalClient::start() {
        resolver_.async_resolve(url_.host, url_.port, beast::bind_front_handler(&SignalClient::OnResolve, this));
    }

    void SignalClient::Disconnect() {
        if (!connected_)
            return;

        connected_ = false;
        work_guard_.reset();
        //m_IOContext.stop();
        websocket_.close(websocket::close_code::normal); // TODO Close should be async
    }

    void SignalClient::Send(SignalRequest req) {
        write_queue_.emplace(req);
    }

    void SignalClient::OnResolve(beast::error_code ec, tcp::resolver::results_type results) {
        if (ec)
            throw std::runtime_error{"SignalClient::OnResolve - " + ec.message()};

        auto &layer = beast::get_lowest_layer(websocket_);
        layer.expires_after(std::chrono::seconds(15));
        layer.async_connect(results, beast::bind_front_handler(&SignalClient::OnConnect, this));
    }

    void SignalClient::OnConnect(beast::error_code ec, tcp::resolver::results_type::endpoint_type ep) {
        if (ec)
            throw std::runtime_error{"SignalClient::OnConnect - " + ec.message()};

        beast::get_lowest_layer(websocket_).expires_never();
        websocket_.set_option(websocket::stream_base::timeout::suggested(beast::role_type::client));

        websocket_.async_handshake(url_.host, "/rtc?access_token=" + token_ + "&protocol=7",
                                    beast::bind_front_handler(&SignalClient::OnHandshake, this));
    }

    void SignalClient::OnHandshake(beast::error_code ec) {
        if (ec)
            throw std::runtime_error{
                    "SignalClient::OnHandshake - " + ec.message()}; // TODO Callback for handling errors

        connected_ = true;
        spdlog::info("Connected to Websocket");
    }

    void SignalClient::OnRead(beast::error_code ec, std::size_t bytesTransferred) {
        reading_ = false;

        if (ec)
            throw std::runtime_error{"SignalClient::OnRead - " + ec.message()};

        SignalResponse res{};
        if (res.ParseFromArray(read_buffer_.cdata().data(), bytesTransferred)) {
            spdlog::info("Received SignalResponse {}", bytesTransferred);
            read_queue_.emplace(res);
        } else {
            spdlog::error("Failed to decode signal message");
        }

        read_buffer_.clear();
    }

    void SignalClient::OnWrite(beast::error_code ec, std::size_t bytesTransferred) {
        writing_ = false;

        if (ec)
            throw std::runtime_error{"SignalClient::OnWrite - " + ec.message()};
    }
} // livekit