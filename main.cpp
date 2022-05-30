#include <iostream>
#include "src/rtc_engine.h"
#include "spdlog/spdlog.h"
#include <thread>

int main() {
    spdlog::info("Starting LiveKit...");

    livekit::RTCEngine engine;
    engine.Join("ws://localhost:7880", "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE2NTY1MTUxOTcsImlzcyI6IkFQSUNrSG04M01oZ2hQeCIsIm5iZiI6MTY1MzkyMzE5Nywic3ViIjoidGVzdGlkZW50aXR5IiwidmlkZW8iOnsicm9vbSI6InRlc3Ryb29tIiwicm9vbUpvaW4iOnRydWV9fQ.M6gIwp_GBVLkE5NwQjGUykn9GDIGIq57Php0LYAk2F8");

    while(true){
        engine.Update();
    }

    return EXIT_SUCCESS;
}
