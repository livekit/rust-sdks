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

// Minimal stand-in for WebRTC's rtc_base/checks.h, so that standalone tests
// can compile native sources without libwebrtc. Add macros as sources need
// them. Unlike in release builds of libwebrtc, DCHECKs are enforced unless
// NDEBUG is defined. Test include paths only; never use in production builds.

#ifndef WEBRTC_SYS_TEST_SHIMS_RTC_BASE_CHECKS_H_
#define WEBRTC_SYS_TEST_SHIMS_RTC_BASE_CHECKS_H_

#include <cassert>

#define RTC_DCHECK_LT(a, b) assert((a) < (b))

#endif  // WEBRTC_SYS_TEST_SHIMS_RTC_BASE_CHECKS_H_
