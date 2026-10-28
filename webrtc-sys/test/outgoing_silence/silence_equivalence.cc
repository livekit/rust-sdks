// Investigative proof only: the baseline packet trace is an ORACLE for
// deadlines. This is not a scheduler implementation and cannot be shipped as
// one.
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <iostream>
#include <map>
#include <memory>
#include <set>
#include <stdexcept>
#include <vector>

#include "api/audio_codecs/opus/audio_encoder_opus.h"
#include "api/environment/environment_factory.h"
#include "rtc_base/buffer.h"
namespace {
constexpr int kSamplesPerFrame = 480;
enum class Mode { Continuous, DeferredOracle, Frozen };

void Require(bool condition, const char* message) {
  if (!condition)
    throw std::runtime_error(message);
}

// Deterministic voiced-like PCM; no recordings or external fixture required.
std::vector<int16_t> MakeSignal() {
  constexpr double pi = 3.14159265358979323846;
  std::vector<int16_t> signal(96000);
  for (size_t i = 0; i < signal.size(); ++i) {
    const double t = static_cast<double>(i) / 48000;
    const double envelope = 0.6 + 0.4 * std::sin(2 * pi * 3 * t);
    signal[i] =
        static_cast<int16_t>(envelope * (8000 * std::sin(2 * pi * 125 * t) +
                                         4000 * std::sin(2 * pi * 250 * t) +
                                         2000 * std::sin(2 * pi * 500 * t)));
  }
  return signal;
}

struct Packet {
  uint32_t timestamp;
  int frame;
  std::vector<uint8_t> bytes;
  bool operator==(const Packet&) const = default;
};
struct Trace {
  std::vector<bool> silence;
  std::set<int> controls;
};
struct Result {
  std::vector<Packet> packets;
  int wakes = 0;
  int silence_wakes = 0;
  int max_batch = 0;
};
static std::unique_ptr<webrtc::AudioEncoder>
Encoder(const webrtc::Environment& env, int rate, int channels, bool dtx) {
  webrtc::AudioEncoderOpusConfig c;
  c.bitrate_bps = rate;
  c.num_channels = channels;
  c.dtx_enabled = dtx;
  c.fec_enabled = true;
  c.complexity = 9;
  c.low_rate_complexity = 9;
  webrtc::AudioEncoderFactory::Options o;
  o.payload_type = 111;
  return webrtc::AudioEncoderOpus::MakeAudioEncoder(env, c, o);
}
static Result Run(const Trace& t,
                  const std::vector<int16_t>& speech,
                  int rate,
                  int channels,
                  bool dtx,
                  Mode mode,
                  const std::set<int>& deadlines) {
  auto env = webrtc::CreateEnvironment();
  auto e = Encoder(env, rate, channels, dtx);
  Require(e != nullptr, "Opus encoder creation failed");
  Result out;
  int next = 0;
  std::vector<int16_t> pcm(kSamplesPerFrame * channels);
  for (int at = 0; at < int(t.silence.size()); at++) {
    // These are logical work events, not measured OS wakeups.
    bool wake = mode == Mode::Continuous || mode == Mode::Frozen || !dtx ||
                !t.silence[at] || t.controls.count(at) || deadlines.count(at) ||
                at + 1 == int(t.silence.size());
    if (!wake)
      continue;
    out.wakes++;
    if (t.silence[at])
      out.silence_wakes++;
    out.max_batch = std::max(out.max_batch, at - next + 1);
    for (; next <= at; next++) {
      if (t.controls.count(next)) {
        // Explicit control events must wake and advance earlier silence first.
        e->OnReceivedTargetAudioBitrate(next % 4000 ? 24000 : 48000);
        e->OnReceivedUplinkPacketLossFraction(.05);
        if (next == 6500)
          e->Reset();
      }
      if (mode == Mode::Frozen && t.silence[next])
        continue;  // Counterexample: RTP time advances but codec state freezes.
      for (int j = 0; j < kSamplesPerFrame; j++)
        for (int ch = 0; ch < channels; ch++)
          pcm[j * channels + ch] =
              t.silence[next] ? 0
                              : speech[(size_t(next) * kSamplesPerFrame + j) %
                                       speech.size()];
      webrtc::Buffer packet;
      auto info = e->Encode(uint32_t(next) * kSamplesPerFrame, pcm, &packet);
      if (!packet.empty())
        out.packets.push_back({info.encoded_timestamp,
                               at,
                               {packet.data(), packet.data() + packet.size()}});
    }
  }
  return out;
}
int RunCases() {
  const auto speech = MakeSignal();
  Trace t;
  for (int gap : {20, 200, 3000, 6000, 20}) {
    t.silence.insert(t.silence.end(), 150, false);
    t.silence.insert(t.silence.end(), gap, true);
  }
  t.silence.insert(t.silence.end(), 150, false);
  t.controls = {1200, 4000, 6500};
  std::cout << "bitrate,channels,dtx,total_frames,baseline_packets,baseline_"
               "events,deferred_events,idle_frames,idle_events,max_batch_"
               "frames,identical_packets,frozen_voice_packets_differ\n";
  for (int rate : {24000, 48000})
    for (int channels : {1, 2})
      for (bool dtx : {false, true}) {
        // Oracle: reference output tells us WHEN to process, not how to predict
        // it.
        auto baseline =
            Run(t, speech, rate, channels, dtx, Mode::Continuous, {});
        std::set<int> oracle;
        for (auto& p : baseline.packets)
          oracle.insert(p.frame);
        auto deferred =
            Run(t, speech, rate, channels, dtx, Mode::DeferredOracle, oracle);
        Require(baseline.packets == deferred.packets,
                "Deferred packets differ in payload, RTP timestamp, or "
                "emission frame");
        auto frozen = Run(t, speech, rate, channels, dtx, Mode::Frozen, {});
        std::map<uint32_t, std::vector<uint8_t>> frozen_packets;
        for (auto& p : frozen.packets)
          frozen_packets[p.timestamp] = p.bytes;
        int differs = 0;
        for (auto& p : baseline.packets)
          if (!t.silence[p.timestamp / kSamplesPerFrame]) {
            auto found = frozen_packets.find(p.timestamp);
            if (found == frozen_packets.end() || found->second != p.bytes)
              differs++;
          }
        Require(differs > 0, "Frozen-state counterexample did not differ");
        std::cout << rate << "," << channels << "," << dtx << ","
                  << t.silence.size() << "," << baseline.packets.size() << ","
                  << baseline.wakes << "," << deferred.wakes << ","
                  << std::count(t.silence.begin(), t.silence.end(), true) << ","
                  << deferred.silence_wakes << "," << deferred.max_batch
                  << ",1," << differs << std::endl;
      }
  return 0;
}
}  // namespace

int main() {
  try {
    return RunCases();
  } catch (const std::exception& error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
}
