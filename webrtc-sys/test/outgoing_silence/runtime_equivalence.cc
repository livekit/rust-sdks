/*
 * Copyright 2026 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "api/audio_codecs/opus/audio_encoder_opus.h"
#include "api/environment/environment_factory.h"
#include "audio/silence_encoder_task_queue.h"
#include "modules/audio_coding/codecs/red/audio_encoder_copy_red.h"
#include "rtc_base/buffer.h"

#include <algorithm>
#include <cmath>
#include <deque>
#include <iostream>
#include <set>
#include <stdexcept>
#include <vector>

void Require(bool ok, const char* message) {
  if (!ok)
    throw std::runtime_error(message);
}

class ManualQueue : public webrtc::TaskQueueBase {
 public:
  int posts = 0;
  std::deque<absl::AnyInvocable<void() &&>> tasks;
  void Run() {
    CurrentTaskQueueSetter current(this);
    while (!tasks.empty()) {
      auto task = std::move(tasks.front());
      tasks.pop_front();
      std::move(task)();
    }
  }
  void Delete() override { tasks.clear(); }
  ~ManualQueue() override = default;

 protected:
  void PostTaskImpl(absl::AnyInvocable<void() &&> task,
                    const PostTaskTraits&,
                    const webrtc::Location&) override {
    ++posts;
    tasks.push_back(std::move(task));
  }
  void PostDelayedTaskImpl(absl::AnyInvocable<void() &&>,
                           webrtc::TimeDelta,
                           const PostDelayedTaskTraits&,
                           const webrtc::Location&) override {
    throw std::runtime_error("Scheduler must not introduce timers");
  }
};

struct Leaf {
  size_t bytes;
  uint32_t timestamp;
  int payload_type;
  bool speech;
  bool send_even_if_empty;
  int encoder_type;
  bool operator==(const Leaf&) const = default;
};
struct Packet {
  uint32_t timestamp;
  int emitted_at;
  int payload_type;
  bool speech;
  bool send_even_if_empty;
  std::vector<uint8_t> bytes;
  std::vector<Leaf> redundant;
  bool operator==(const Packet&) const = default;
};
struct Result {
  std::vector<Packet> packets;
  int posts;
};

Result Run(const std::vector<bool>& silence,
           int bitrate,
           int channels,
           bool dtx,
           int ptime,
           bool coalesce,
           int backlog,
           bool red) {
  webrtc::SimulatedClock clock(0);
  auto env = webrtc::CreateEnvironment(&clock);
  webrtc::AudioEncoderOpusConfig config;
  config.bitrate_bps = bitrate;
  config.num_channels = channels;
  config.dtx_enabled = dtx;
  config.fec_enabled = true;
  config.complexity = config.low_rate_complexity = 9;
  config.frame_size_ms = ptime;
  webrtc::AudioEncoderFactory::Options options;
  options.payload_type = 111;
  auto encoder =
      webrtc::AudioEncoderOpus::MakeAudioEncoder(env, config, options);
  Require(encoder != nullptr, "Encoder creation failed");
  if (red) {
    webrtc::AudioEncoderCopyRed::Config red_config;
    red_config.payload_type = 112;
    red_config.speech_encoder = std::move(encoder);
    encoder = std::make_unique<webrtc::AudioEncoderCopyRed>(
        std::move(red_config), env.field_trials());
  }
  ManualQueue queue;
  Result result;
  int at = -1;
  auto process = [&](std::unique_ptr<webrtc::AudioFrame> frame,
                     webrtc::Timestamp) {
    webrtc::Buffer bytes;
    auto info = encoder->Encode(frame->timestamp_, frame->data_view(), &bytes);
    if (info.encoded_bytes || info.send_even_if_empty) {
      result.packets.push_back({info.encoded_timestamp,
                                at,
                                info.payload_type,
                                info.speech,
                                info.send_even_if_empty,
                                {bytes.data(), bytes.data() + bytes.size()},
                                {}});
      for (const auto& leaf : info.redundant) {
        result.packets.back().redundant.push_back(
            {leaf.encoded_bytes, leaf.encoded_timestamp, leaf.payload_type,
             leaf.speech, leaf.send_even_if_empty,
             static_cast<int>(leaf.encoder_type)});
      }
    }
  };
  webrtc::voe::SilenceEncoderTaskQueue scheduler(&queue, &clock, process, [&] {
    return encoder->Num10MsFramesUntilSilenceWork();
  });
  // Initialize the capability after the encoder has been installed, as
  // ChannelSend does.
  if (coalesce) {
    scheduler.PostTask([] {});
    queue.Run();
  }
  const std::set<int> controls{1200, 4000, 6500};
  for (at = 0; at < static_cast<int>(silence.size()); ++at) {
    clock.AdvanceTimeMilliseconds(10);
    if (controls.count(at)) {
      auto control = [&, event = at] {
        encoder->OnReceivedTargetAudioBitrate(event % 4000 ? 24000 : 48000);
        encoder->OnReceivedUplinkPacketLossFraction(event == 4000 ? .15f
                                                                  : .05f);
        if (event == 6500)
          encoder->Reset();
      };
      if (coalesce)
        scheduler.PostTask(control);
      else
        queue.PostTask(control);
      queue.Run();
    }
    std::vector<int16_t> pcm(480 * channels);
    for (int j = 0; j < 480; ++j) {
      double t = (at * 480.0 + j) / 48000;
      int16_t value =
          silence[at] ? 0
                      : static_cast<int16_t>(
                            8000 * std::sin(2 * 3.141592653589793 * 125 * t) +
                            4000 * std::sin(2 * 3.141592653589793 * 500 * t));
      for (int ch = 0; ch < channels; ++ch)
        pcm[j * channels + ch] = value;
    }
    auto frame = std::make_unique<webrtc::AudioFrame>();
    frame->UpdateFrame(at * 480, pcm.data(), 480, 48000,
                       webrtc::AudioFrame::kNormalSpeech,
                       webrtc::AudioFrame::kVadUnknown, channels);
    if (coalesce)
      scheduler.PostFrame(std::move(frame), silence[at]);
    else
      queue.PostTask([&, frame = std::move(frame)]() mutable {
        process(std::move(frame), webrtc::Timestamp::Zero());
      });
    if (at % backlog == backlog - 1)
      queue.Run();
  }
  if (coalesce)
    scheduler.PostTask([] {});
  queue.Run();
  result.posts = queue.posts;
  return result;
}

int main() {
  std::vector<bool> silence;
  for (int gap : {20, 200, 3000, 6000, 20}) {
    silence.insert(silence.end(), 150, false);
    silence.insert(silence.end(), gap, true);
  }
  silence.insert(silence.end(), 150, false);
  std::cout << "bitrate,channels,dtx,ptime,backlog,red,reference_posts,"
               "scheduler_posts,completed_outputs,identical\n";
  for (int rate : {24000, 48000})
    for (int channels : {1, 2})
      for (bool dtx : {false, true})
        for (int ptime : {10, 20, 40, 60, 120})
          for (int backlog : {1, 3})
            for (bool red : {false, true}) {
              if (red && ptime != 20)
                continue;
              auto reference =
                  Run(silence, rate, channels, dtx, ptime, false, backlog, red);
              auto actual =
                  Run(silence, rate, channels, dtx, ptime, true, backlog, red);
              Require(reference.packets == actual.packets,
                      "Runtime scheduler output mismatch");
              std::cout << rate << ',' << channels << ',' << dtx << ',' << ptime
                        << ',' << backlog << ',' << red << ','
                        << reference.posts << ',' << actual.posts << ','
                        << actual.packets.size() << ",1\n";
            }
}
