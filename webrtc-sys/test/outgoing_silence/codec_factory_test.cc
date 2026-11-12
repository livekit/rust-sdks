// Copyright 2026 LiveKit, Inc.
// Licensed under the Apache License, Version 2.0.

#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/environment/environment_factory.h"
#include "rtc_base/buffer.h"

#include <iostream>
#include <set>
#include <stdexcept>
#include <vector>

int main() {
  auto env = webrtc::CreateEnvironment();
  auto factory = webrtc::CreateBuiltinAudioEncoderFactory();
  std::set<std::string> codecs;
  for (const auto& spec : factory->GetSupportedEncoders()) {
    auto encoder = factory->Create(env, spec.format, {.payload_type = 111});
    if (!encoder || !factory->QueryAudioEncoder(spec.format))
      throw std::runtime_error("Advertised codec cannot be created");
    if (spec.format.name != "opus" &&
        encoder->Num10MsFramesUntilSilenceWork() != 0)
      throw std::runtime_error("Unsupported codec advertised a silence budget");
    codecs.insert(spec.format.name);
    std::vector<int16_t> pcm(
        encoder->SampleRateHz() / 100 * encoder->NumChannels(), 1000);
    size_t packets = 0;
    for (int i = 0; i < 20; ++i) {
      webrtc::Buffer bytes;
      encoder->Encode(i * encoder->RtpTimestampRateHz() / 100, pcm, &bytes);
      packets += !bytes.empty();
    }
    if (!packets)
      throw std::runtime_error("Advertised codec produced no packets");
    std::cout << spec.format.name << " packets=" << packets << '\n';
  }
  for (const char* expected : {"opus", "G722", "PCMU", "PCMA"})
    if (!codecs.count(expected))
      throw std::runtime_error("Expected built-in codec missing");
}
