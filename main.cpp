#include <iostream>
#include "src/signal_client.h"
#include "spdlog/spdlog.h"

int main() {
    spdlog::info("Starting LiveKit...");

    livekit::SignalClient client;
    client.Connect("ws://localhost:7880", "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE2NTMzMTY2MjYsImlzcyI6IkFQSUNrSG04M01oZ2hQeCIsIm5iZiI6MTY1MDcyNDYyNiwic3ViIjoidGVzdCIsInZpZGVvIjp7InJvb20iOiJ0ZXN0cm9vbSIsInJvb21Kb2luIjp0cnVlfX0.I3Q5W4pk1kUNguGEJ4m95nE8hl8cPCliXBtF9hCt-Wg");

    while(true){
        client.Update();
    }

    return EXIT_SUCCESS;
}
