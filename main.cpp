#include <iostream>
#include <boost/regex.hpp>
#include "src/signal_client.h"

int main() {
    std::cout << "Hello, World!" << std::endl;

    livekit::SignalClient client;
    client.Connect("ws://localhost:7880/rtc", "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE2NTMzMTY2MjYsImlzcyI6IkFQSUNrSG04M01oZ2hQeCIsIm5iZiI6MTY1MDcyNDYyNiwic3ViIjoidGVzdCIsInZpZGVvIjp7InJvb20iOiJ0ZXN0cm9vbSIsInJvb21Kb2luIjp0cnVlfX0.I3Q5W4pk1kUNguGEJ4m95nE8hl8cPCliXBtF9hCt-Wg");

    return 0;
}