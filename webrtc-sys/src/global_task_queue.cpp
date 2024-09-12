#include "livekit/global_task_queue.h"

#include "api/task_queue/default_task_queue_factory.h"
#include "api/task_queue/task_queue_factory.h"

namespace livekit {

webrtc::TaskQueueFactory* GetGlobalTaskQueueFactory() {
  static std::unique_ptr<webrtc::TaskQueueFactory> global_task_queue_factory =
      webrtc::CreateDefaultTaskQueueFactory();
  return global_task_queue_factory.get();
}

}  // namespace livekit
