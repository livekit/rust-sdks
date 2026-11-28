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

#include "api/task_queue/default_task_queue_factory.h"
#include "audio/silence_encoder_task_queue.h"
#include "rtc_base/event.h"

#include <atomic>
#include <deque>
#include <iostream>
#include <stdexcept>
#include <thread>
#include <vector>

void Check(bool ok) {
  if (!ok)
    throw std::runtime_error("Silence scheduler invariant failed");
}
class ManualQueue : public webrtc::TaskQueueBase {
 public:
  int posts = 0;
  std::deque<absl::AnyInvocable<void() &&>> tasks;
  void One() {
    CurrentTaskQueueSetter current(this);
    Check(!tasks.empty());
    auto task = std::move(tasks.front());
    tasks.pop_front();
    std::move(task)();
  }
  void Run() {
    while (!tasks.empty())
      One();
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
    throw std::runtime_error("Unexpected timer");
  }
};
std::unique_ptr<webrtc::AudioFrame> Frame(int id) {
  auto frame = std::make_unique<webrtc::AudioFrame>();
  frame->timestamp_ = id;
  return frame;
}
void ManualChecks() {
  ManualQueue queue;
  std::vector<int> order;
  size_t budget = 2;
  webrtc::voe::SilenceEncoderTaskQueue scheduler(
      &queue, webrtc::Clock::GetRealTimeClock(),
      [&](auto frame, auto) { order.push_back(frame->timestamp_); },
      [&] { return budget; });
  scheduler.PostTask([] {});
  queue.Run();
  int before = queue.posts;
  scheduler.PostFrame(Frame(1), true);
  Check(queue.posts == before);
  scheduler.PostTask([&] { order.push_back(-1); });
  queue.Run();
  Check(order == std::vector<int>({1, -1}));
  scheduler.PostFrame(Frame(2), true);
  scheduler.PostFrame(Frame(3), false);
  queue.Run();
  Check(order == std::vector<int>({1, -1, 2, 3}));
  // An old silent batch must not publish a budget over newer queued input.
  scheduler.PostTask([] {});
  queue.Run();
  scheduler.PostFrame(Frame(4), true);
  scheduler.PostFrame(Frame(5), true);
  scheduler.PostFrame(Frame(6), false);
  queue.One();
  before = queue.posts;
  scheduler.PostFrame(Frame(7), true);
  Check(queue.posts == before + 1);
  queue.Run();
  Check(order == std::vector<int>({1, -1, 2, 3, 4, 5, 6, 7}));
  scheduler.PostTask([&] { budget = 1; });
  queue.Run();
  before = queue.posts;
  for (int i = 0; i < 10; ++i) {
    scheduler.PostFrame(Frame(8 + i), true);
    queue.Run();
  }
  Check(queue.posts == before + 10);
  int queries = 0;
  ManualQueue unsupported_queue;
  webrtc::voe::SilenceEncoderTaskQueue unsupported(
      &unsupported_queue, webrtc::Clock::GetRealTimeClock(), [](auto, auto) {},
      [&] {
        ++queries;
        return size_t{0};
      });
  unsupported.PostTask([] {});
  unsupported_queue.Run();
  for (int i = 0; i < 20; ++i) {
    unsupported.PostFrame(Frame(i), true);
    unsupported_queue.Run();
  }
  Check(queries == 1);
  unsupported.PostTask([] {});
  unsupported_queue.Run();
  Check(queries == 2);
  unsupported_queue.Delete();
  queue.Delete();
}
struct State {
  std::atomic<int> frames = 0;
  std::atomic<int> controls = 0;
};
void ThreadedChecks() {
  auto factory = webrtc::CreateDefaultTaskQueueFactory();
  for (int cycle = 0; cycle < 32; ++cycle) {
    auto state = std::make_shared<State>();
    std::weak_ptr<State> weak = state;
    auto queue = factory->CreateTaskQueue(
        "SilenceSchedulerTest", webrtc::TaskQueueFactory::Priority::NORMAL);
    auto scheduler = std::make_unique<webrtc::voe::SilenceEncoderTaskQueue>(
        queue.get(), webrtc::Clock::GetRealTimeClock(),
        [state](auto, auto) { ++state->frames; }, [] { return 2; });
    std::thread media([&] {
      for (int i = 0; i < 200; ++i)
        scheduler->PostFrame(Frame(i), true);
    });
    std::thread controls([&] {
      for (int i = 0; i < 100; ++i)
        scheduler->PostTask([state] { ++state->controls; });
    });
    media.join();
    controls.join();
    webrtc::Event done;
    scheduler->PostTask([&] { done.Set(); });
    Check(done.Wait(webrtc::TimeDelta::Seconds(5)));
    queue.reset();
    Check(state->frames == 200 && state->controls == 100);
    scheduler.reset();
    state.reset();
    Check(weak.expired());
  }
}
int main() {
  ManualChecks();
  ThreadedChecks();
  std::cout
      << "PASS: control ordering, speech preemption, conservative fallback, "
         "stale budgets, concurrent admission and 32 joined teardown cycles\n";
}
