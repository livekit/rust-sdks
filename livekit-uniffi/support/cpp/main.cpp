#include <livekit_net.hpp>
#include <livekit_uniffi.hpp>

#include <atomic>
#include <cassert>
#include <chrono>
#include <memory>
#include <string>
#include <thread>
#include <vector>

using namespace std::chrono_literals;

class SmokeHttpClient final : public livekit_net::HttpClientForeign {
public:
    explicit SmokeHttpClient(std::shared_ptr<std::atomic<int>> cancellation_count):
        cancellation_count_(std::move(cancellation_count)) {}

    uniffi::ForeignFuture<livekit_net::HttpResponse> request(
        const livekit_net::HttpMethod &,
        const std::string &url,
        const std::vector<livekit_net::Header> &,
        std::optional<std::vector<uint8_t>>
    ) override {
        if (url == "smoke://pending") {
            auto cancellation_count = cancellation_count_;
            return uniffi::ForeignFuture<livekit_net::HttpResponse>(
                [cancellation_count](auto, auto) {
                    return [cancellation_count] { cancellation_count->fetch_add(1); };
                }
            );
        }

        if (url == "smoke://failure") {
            return uniffi::ForeignFuture<livekit_net::HttpResponse>(
                [](auto, auto failure) {
                    failure(std::make_exception_ptr(
                        livekit_net::transport_error::Timeout("smoke timeout")
                    ));
                    return [] {};
                }
            );
        }

        return uniffi::ForeignFuture<livekit_net::HttpResponse>(
            [](auto success, auto) {
                success(livekit_net::HttpResponse{204, {}, {'o', 'k'}});
                return [] {};
            }
        );
    }

private:
    std::shared_ptr<std::atomic<int>> cancellation_count_;
};

int main() {
    assert(!livekit_uniffi::build_version().empty());

    auto cancellation_count = std::make_shared<std::atomic<int>>(0);
    auto client = std::make_shared<SmokeHttpClient>(cancellation_count);
    livekit_net::set_http_client(client);
    assert(livekit_net::has_http_client());

    auto response = livekit_net::self_test_http_get("smoke://success").get();
    assert(response.status == 204);
    assert(response.body == std::vector<uint8_t>({'o', 'k'}));

    try {
        livekit_net::self_test_http_get("smoke://failure").get();
        assert(false && "typed transport error was not propagated");
    } catch (const livekit_net::transport_error::Timeout &) {
    }

    auto pending = livekit_net::self_test_http_get("smoke://pending");
    pending.cancel();
    try {
        pending.get();
        assert(false && "cancelled future completed successfully");
    } catch (const uniffi::AsyncCancelledError &) {
    }

    for (int i = 0; i < 100 && cancellation_count->load() == 0; ++i) {
        std::this_thread::sleep_for(1ms);
    }
    assert(cancellation_count->load() == 1);

    uniffi::shutdown_async_dispatcher();
    return 0;
}
