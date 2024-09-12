#pragma once

#include "api/task_queue/task_queue_factory.h"

namespace livekit {

webrtc::TaskQueueFactory* GetGlobalTaskQueueFactory();

} // namespace livekit
